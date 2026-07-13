use anyhow::{Context, Result};
use hf_hub::api::sync::Api;
use lazy_static::lazy_static;
use std::sync::Mutex;
use tokenizers::Tokenizer;
use tract_onnx::prelude::*;

const REPO_ID: &str = "Xenova/paraphrase-multilingual-MiniLM-L12-v2";

lazy_static! {
    static ref EMBEDDING_ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
}

struct Engine {
    model: std::sync::Arc<TypedSimplePlan>,
    tokenizer: Tokenizer,
}

fn init_engine() -> Result<Engine> {
    let api = Api::new().context("Failed to initialize hf_hub Api")?;
    let repo = api.model(REPO_ID.to_string());

    // Download weights and tokenizer
    let model_path = repo
        .get("onnx/model.onnx")
        .context("Failed to download model.onnx")?;
    let tokenizer_path = repo
        .get("tokenizer.json")
        .context("Failed to download tokenizer.json")?;

    let mut tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Tokenizer error: {}", e))?;

    // MiniLM has a strict 512 token limit. We MUST truncate to prevent ONNX panics on long inputs.
    let truncation = tokenizers::utils::truncation::TruncationParams {
        max_length: 512,
        direction: tokenizers::utils::truncation::TruncationDirection::Right,
        strategy: tokenizers::utils::truncation::TruncationStrategy::LongestFirst,
        stride: 0,
    };
    tokenizer.with_truncation(Some(truncation)).unwrap();

    // Load ONNX model
    let model = tract_onnx::onnx()
        .model_for_path(model_path)?
        .into_optimized()?
        .into_runnable()?;

    Ok(Engine { model, tokenizer })
}

pub(crate) fn embed_local(text: &str) -> Result<Vec<f32>> {
    let mut guard = EMBEDDING_ENGINE.lock().unwrap();
    if guard.is_none() {
        *guard = Some(init_engine()?);
    }

    let engine = guard.as_ref().unwrap();

    // 1. Tokenize
    let encoding = engine
        .tokenizer
        .encode(text, true)
        .map_err(|e| anyhow::anyhow!("Tokenization error: {}", e))?;
    let input_ids = encoding.get_ids();
    let attention_mask = encoding.get_attention_mask();
    let token_type_ids = encoding.get_type_ids();

    let seq_len = input_ids.len();

    // 2. Prepare tensors
    let input_ids_tensor = tract_ndarray::Array2::from_shape_vec(
        (1, seq_len),
        input_ids.iter().map(|&x| x as i64).collect(),
    )?
    .into_tensor();
    let attention_mask_tensor = tract_ndarray::Array2::from_shape_vec(
        (1, seq_len),
        attention_mask.iter().map(|&x| x as i64).collect(),
    )?
    .into_tensor();
    let token_type_ids_tensor = tract_ndarray::Array2::from_shape_vec(
        (1, seq_len),
        token_type_ids.iter().map(|&x| x as i64).collect(),
    )?
    .into_tensor();

    // 3. Run model
    // The inputs depend on the specific ONNX graph signature.
    // For paraphrase-multilingual-MiniLM-L12-v2 from Xenova:
    // usually inputs are: input_ids, attention_mask, token_type_ids
    let result = engine.model.run(tvec!(
        input_ids_tensor.into(),
        attention_mask_tensor.into(),
        token_type_ids_tensor.into()
    ))?;

    // Result is usually a tuple of tensors. The first one is typically last_hidden_state (1, seq_len, 384)
    let tensor = result[0].clone().into_tensor();
    let slice = unsafe { tensor.as_slice_unchecked::<f32>() };

    // 4. Mean Pooling
    // sum(token_embeddings * attention_mask) / sum(attention_mask)
    let mut pooled = vec![0.0f32; 384];
    let mut sum_mask = 0.0f32;

    for i in 0..seq_len {
        let mask = attention_mask[i] as f32;
        sum_mask += mask;
        for j in 0..384 {
            pooled[j] += slice[i * 384 + j] * mask;
        }
    }

    if sum_mask > 0.0 {
        for value in &mut pooled {
            *value /= sum_mask;
        }
    }

    // 5. L2 Normalization
    let mut norm = 0.0f32;
    for val in &pooled {
        norm += val * val;
    }
    norm = norm.sqrt();
    if norm > 0.0 {
        for val in &mut pooled {
            *val /= norm;
        }
    }

    Ok(pooled)
}

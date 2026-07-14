#[cfg(feature = "local-embeddings")]
mod enabled {
    use crate::app::model_artifact::download_hf_model;
    use anyhow::{Context, Result, bail};
    use hf_hub::api::sync::Api;
    use std::sync::{Arc, Mutex, OnceLock};
    use tokenizers::Tokenizer;
    use tract_onnx::prelude::*;

    const REPO_ID: &str = "Xenova/paraphrase-multilingual-MiniLM-L12-v2";
    const REVISION: &str = "b9e20f0c6dd7e5f88c72259765011e6a3a9196bc";
    const MODEL_FILE: &str = "onnx/model.onnx";
    const MODEL_SHA256: &str = "6684d88a9bde425c73a9cd960db19c8aff3cf7da8e0fc81076a3610ed942c1bb";
    const TOKENIZER_FILE: &str = "tokenizer.json";
    const TOKENIZER_SHA256: &str =
        "b60b6b43406a48bf3638526314f3d232d97058bc93472ff2de930d43686fa441";

    static EMBEDDING_ENGINE: OnceLock<Engine> = OnceLock::new();
    static EMBEDDING_ENGINE_INIT: Mutex<()> = Mutex::new(());

    struct Engine {
        model: Arc<TypedSimplePlan>,
        tokenizer: Tokenizer,
    }

    fn init_engine() -> Result<Engine> {
        let api = Api::new().context("failed to initialize hf_hub Api")?;
        let model_path =
            download_hf_model(&api, REPO_ID, REVISION, MODEL_FILE, Some(MODEL_SHA256))?;
        let tokenizer_path = download_hf_model(
            &api,
            REPO_ID,
            REVISION,
            TOKENIZER_FILE,
            Some(TOKENIZER_SHA256),
        )?;

        let mut tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|error| anyhow::anyhow!("tokenizer error: {error}"))?;
        tokenizer
            .with_truncation(Some(tokenizers::utils::truncation::TruncationParams {
                max_length: 512,
                direction: tokenizers::utils::truncation::TruncationDirection::Right,
                strategy: tokenizers::utils::truncation::TruncationStrategy::LongestFirst,
                stride: 0,
            }))
            .map_err(|error| anyhow::anyhow!("tokenizer truncation error: {error}"))?;

        let model = tract_onnx::onnx()
            .model_for_path(model_path)?
            .into_optimized()?
            .into_runnable()?;
        Ok(Engine { model, tokenizer })
    }

    fn engine() -> Result<&'static Engine> {
        if let Some(engine) = EMBEDDING_ENGINE.get() {
            return Ok(engine);
        }
        let _init_guard = EMBEDDING_ENGINE_INIT
            .lock()
            .map_err(|_| anyhow::anyhow!("embedding engine initialization lock poisoned"))?;
        if EMBEDDING_ENGINE.get().is_none() {
            EMBEDDING_ENGINE
                .set(init_engine()?)
                .map_err(|_| anyhow::anyhow!("embedding engine initialized concurrently"))?;
        }
        EMBEDDING_ENGINE
            .get()
            .context("embedding engine was not initialized")
    }

    pub(crate) fn embed_local(text: &str) -> Result<Vec<f32>> {
        let engine = engine()?;
        let encoding = engine
            .tokenizer
            .encode(text, true)
            .map_err(|error| anyhow::anyhow!("tokenization error: {error}"))?;
        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        let token_type_ids = encoding.get_type_ids();
        let seq_len = input_ids.len();
        if seq_len == 0 {
            bail!("tokenizer produced an empty sequence");
        }

        let input_ids_tensor = tract_ndarray::Array2::from_shape_vec(
            (1, seq_len),
            input_ids.iter().map(|&value| value as i64).collect(),
        )?
        .into_tensor();
        let attention_mask_tensor = tract_ndarray::Array2::from_shape_vec(
            (1, seq_len),
            attention_mask.iter().map(|&value| value as i64).collect(),
        )?
        .into_tensor();
        let token_type_ids_tensor = tract_ndarray::Array2::from_shape_vec(
            (1, seq_len),
            token_type_ids.iter().map(|&value| value as i64).collect(),
        )?
        .into_tensor();

        let result = engine.model.run(tvec!(
            input_ids_tensor.into(),
            attention_mask_tensor.into(),
            token_type_ids_tensor.into()
        ))?;
        let tensor = result
            .first()
            .context("embedding model returned no output tensors")?
            .clone()
            .into_tensor();
        mean_pool_and_normalize(&tensor, attention_mask)
    }

    fn mean_pool_and_normalize(tensor: &Tensor, attention_mask: &[u32]) -> Result<Vec<f32>> {
        let view = tensor.to_plain_array_view::<f32>()?;
        let shape = view.shape();
        if shape.len() != 3 || shape[0] != 1 || shape[1] != attention_mask.len() {
            bail!(
                "unexpected embedding tensor shape {:?}; expected [1, {}, dimension]",
                shape,
                attention_mask.len()
            );
        }
        let dimension = shape[2];
        if dimension == 0 {
            bail!("embedding model returned a zero-width vector");
        }

        let mut pooled = vec![0.0_f32; dimension];
        let mut sum_mask = 0.0_f32;
        for (token_index, &mask) in attention_mask.iter().enumerate() {
            let mask = mask as f32;
            sum_mask += mask;
            for dimension_index in 0..dimension {
                pooled[dimension_index] += view[[0, token_index, dimension_index]] * mask;
            }
        }
        if sum_mask == 0.0 {
            bail!("embedding attention mask contains no active tokens");
        }
        for value in &mut pooled {
            *value /= sum_mask;
        }

        let norm = pooled.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm == 0.0 {
            bail!("embedding model returned a zero vector");
        }
        for value in &mut pooled {
            *value /= norm;
        }
        Ok(pooled)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn pooling_derives_dimension_and_respects_attention_mask() -> Result<()> {
            let tensor = tract_ndarray::Array3::from_shape_vec(
                (1, 2, 3),
                vec![1.0_f32, 0.0, 0.0, 0.0, 9.0, 0.0],
            )?
            .into_tensor();

            let embedding = mean_pool_and_normalize(&tensor, &[1, 0])?;

            assert_eq!(embedding, vec![1.0, 0.0, 0.0]);
            Ok(())
        }
    }
}

#[cfg(feature = "local-embeddings")]
pub(crate) use enabled::embed_local;

#[cfg(not(feature = "local-embeddings"))]
pub(crate) fn embed_local(_text: &str) -> anyhow::Result<Vec<f32>> {
    anyhow::bail!(
        "local embedding provider requires building dukememory with --features local-embeddings"
    )
}

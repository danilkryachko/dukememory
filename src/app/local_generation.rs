#![cfg(feature = "local-generation")]

use crate::app::model_artifact::download_hf_model;
use anyhow::{Context, Result, bail};
use hf_hub::api::sync::Api;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::{LogOptions, send_logs_to_tracing};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

const DEFAULT_REPO_ID: &str = "HuggingFaceTB/SmolLM2-360M-Instruct-GGUF";
const DEFAULT_FILE_NAME: &str = "smollm2-360m-instruct-q8_0.gguf";
const DEFAULT_REVISION: &str = "2633adad3eb0aec759aec7f41db367d974571ecf";
const DEFAULT_SHA256: &str = "48ab3034d0dd401fbc721eb1df3217902fee7dab9078992d66431f09b7750201";
const Q4_REPO_ID: &str = "bartowski/SmolLM2-360M-Instruct-GGUF";
const Q4_FILE_NAME: &str = "SmolLM2-360M-Instruct-Q4_0.gguf";
const Q4_REVISION: &str = "56e32585e089f5cb84f0e4f8d64e8319332ea9a3";
const Q4_SHA256: &str = "c3608933eb6e5763b87f769bda40c204dc158333668c7af214644fe39da58627";
const DEFAULT_CONTEXT_TOKENS: u32 = 2048;
const DEFAULT_MAX_NEW_TOKENS: usize = 192;

static GENERATION_ENGINE: OnceLock<Mutex<Option<LocalGenerationEngine>>> = OnceLock::new();

struct LocalGenerationEngine {
    repo_id: String,
    file_name: String,
    model_path: PathBuf,
    model: LlamaModel,
    _backend: LlamaBackend,
}

pub(crate) fn generate_local(
    endpoint: &str,
    model: &str,
    prompt: &str,
    cancellation: &AtomicBool,
    deadline: Instant,
) -> Result<String> {
    ensure_local_generation_active(cancellation, deadline)?;
    let engine_lock = GENERATION_ENGINE.get_or_init(|| Mutex::new(None));
    let mut guard = loop {
        ensure_local_generation_active(cancellation, deadline)?;
        match engine_lock.try_lock() {
            Ok(guard) => break guard,
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
            Err(TryLockError::Poisoned(_)) => bail!("local generation engine lock poisoned"),
        }
    };
    let spec = resolve_model_spec(endpoint, model)?;
    ensure_local_generation_active(cancellation, deadline)?;

    let reload = guard
        .as_ref()
        .map(|engine| {
            engine.repo_id != spec.repo_id
                || engine.file_name != spec.file_name
                || engine.model_path != spec.model_path
        })
        .unwrap_or(true);
    if reload {
        guard.take();
        *guard = Some(init_engine(&spec)?);
        ensure_local_generation_active(cancellation, deadline)?;
    }

    let engine = guard
        .as_ref()
        .context("local generation engine was not initialized")?;
    generate_with_engine(engine, prompt, cancellation, deadline)
}

fn ensure_local_generation_active(cancellation: &AtomicBool, deadline: Instant) -> Result<()> {
    if cancellation.load(Ordering::Acquire) {
        bail!("local generation was cancelled");
    }
    if Instant::now() >= deadline {
        bail!("local generation timed out");
    }
    Ok(())
}

struct LocalModelSpec {
    repo_id: String,
    file_name: String,
    model_path: PathBuf,
}

fn resolve_model_spec(endpoint: &str, model: &str) -> Result<LocalModelSpec> {
    let endpoint = endpoint.trim();
    let model = model.trim();

    if !model.is_empty() {
        let path = PathBuf::from(model);
        if path.exists() {
            return Ok(LocalModelSpec {
                repo_id: "local-file".to_string(),
                file_name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("model.gguf")
                    .to_string(),
                model_path: path,
            });
        }
    }

    let (repo_id, file_name, revision, expected_sha256) = match model {
        "" | "smollm2:360m" | "smollm2:360m-instruct" | "smollm2:360m-instruct-q8_0" => (
            DEFAULT_REPO_ID.to_string(),
            DEFAULT_FILE_NAME.to_string(),
            DEFAULT_REVISION.to_string(),
            Some(DEFAULT_SHA256),
        ),
        "smollm2:360m-instruct-q4_0" => (
            Q4_REPO_ID.to_string(),
            Q4_FILE_NAME.to_string(),
            Q4_REVISION.to_string(),
            Some(Q4_SHA256),
        ),
        value if value.starts_with("hf://") => {
            let (repo_id, file_name, revision) = parse_hf_model_spec(value)?;
            (repo_id, file_name, revision, None)
        }
        value if value.ends_with(".gguf") => {
            let repo = if endpoint.is_empty() || endpoint == "local" {
                DEFAULT_REPO_ID
            } else {
                endpoint
            };
            (
                repo.to_string(),
                value.to_string(),
                "main".to_string(),
                None,
            )
        }
        value if value.contains('/') && (endpoint.is_empty() || endpoint == "local") => (
            value.to_string(),
            DEFAULT_FILE_NAME.to_string(),
            "main".to_string(),
            None,
        ),
        value => {
            let repo = if endpoint.is_empty() || endpoint == "local" {
                DEFAULT_REPO_ID
            } else {
                endpoint
            };
            (
                repo.to_string(),
                value.to_string(),
                "main".to_string(),
                None,
            )
        }
    };

    let api = Api::new().context("failed to initialize hf_hub Api")?;
    let model_path = download_hf_model(&api, &repo_id, &revision, &file_name, expected_sha256)?;

    Ok(LocalModelSpec {
        repo_id,
        file_name,
        model_path,
    })
}

fn parse_hf_model_spec(value: &str) -> Result<(String, String, String)> {
    let spec = value.trim_start_matches("hf://");
    if let Some((repo_spec, file)) = spec.rsplit_once(':')
        && !repo_spec.trim().is_empty()
        && !file.trim().is_empty()
    {
        let (repo, revision) = repo_spec
            .rsplit_once('@')
            .filter(|(repo, revision)| !repo.trim().is_empty() && !revision.trim().is_empty())
            .map(|(repo, revision)| (repo.to_string(), revision.to_string()))
            .unwrap_or_else(|| (repo_spec.to_string(), "main".to_string()));
        return Ok((repo, file.to_string(), revision));
    }
    bail!("expected hf://repo/name[@revision]:file.gguf for local generation model")
}

fn init_engine(spec: &LocalModelSpec) -> Result<LocalGenerationEngine> {
    send_logs_to_tracing(LogOptions::default().with_logs_enabled(false));
    let mut backend = LlamaBackend::init().context("failed to initialize llama.cpp backend")?;
    backend.void_logs();
    let model_params = LlamaModelParams::default().with_n_gpu_layers(0);
    let model = LlamaModel::load_from_file(&backend, &spec.model_path, &model_params)
        .with_context(|| format!("failed to load GGUF model {}", spec.model_path.display()))?;
    Ok(LocalGenerationEngine {
        repo_id: spec.repo_id.clone(),
        file_name: spec.file_name.clone(),
        model_path: spec.model_path.clone(),
        model,
        _backend: backend,
    })
}

fn generate_with_engine(
    engine: &LocalGenerationEngine,
    prompt: &str,
    cancellation: &AtomicBool,
    deadline: Instant,
) -> Result<String> {
    ensure_local_generation_active(cancellation, deadline)?;
    let prompt = render_chat_prompt(&engine.model, prompt);
    let context_tokens = local_context_tokens();
    let mut max_new_tokens = local_max_new_tokens();
    let threads = local_thread_count();
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(context_tokens))
        .with_n_threads(threads)
        .with_n_threads_batch(threads)
        .with_offload_kqv(false)
        .with_op_offload(false);
    let mut ctx = engine
        .model
        .new_context(&engine._backend, ctx_params)
        .context("failed to create llama.cpp context")?;

    let tokens = engine
        .model
        .str_to_token(&prompt, AddBos::Always)
        .context("failed to tokenize local generation prompt")?;
    if tokens.is_empty() {
        bail!("local generation prompt produced no tokens");
    }

    let n_ctx = ctx.n_ctx() as usize;
    if tokens.len() >= n_ctx {
        bail!(
            "local generation prompt is too long: {} tokens exceed context {}",
            tokens.len(),
            n_ctx
        );
    }
    if tokens.len() + max_new_tokens >= n_ctx {
        max_new_tokens = n_ctx.saturating_sub(tokens.len() + 1);
    }
    if max_new_tokens == 0 {
        bail!("local generation context has no room for output tokens");
    }

    let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
    let last_index = (tokens.len() - 1) as i32;
    for (i, token) in (0_i32..).zip(tokens) {
        batch.add(token, i, &[0], i == last_index)?;
    }
    ctx.decode(&mut batch)
        .context("llama.cpp failed to decode prompt")?;
    ensure_local_generation_active(cancellation, deadline)?;

    let target_len = batch.n_tokens() + max_new_tokens as i32;
    let mut n_cur = batch.n_tokens();
    let mut output = String::new();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::temp(0.7),
        LlamaSampler::top_k(40),
        LlamaSampler::top_p(0.95, 1),
        LlamaSampler::dist(42),
    ]);

    while n_cur <= target_len {
        ensure_local_generation_active(cancellation, deadline)?;
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if engine.model.is_eog_token(token) {
            break;
        }

        output.push_str(
            &engine
                .model
                .token_to_piece(token, &mut decoder, true, None)
                .context("failed to decode generated token")?,
        );

        batch.clear();
        batch.add(token, n_cur, &[0], true)?;
        n_cur += 1;
        ctx.decode(&mut batch)
            .context("llama.cpp failed to decode generated token")?;
    }

    let mut tail = String::new();
    let _ = decoder.decode_to_string(&[], &mut tail, true);
    output.push_str(&tail);
    Ok(output.trim().to_string())
}

fn render_chat_prompt(model: &LlamaModel, prompt: &str) -> String {
    let chat_prompt = || -> Result<String> {
        let template = model.chat_template(None)?;
        let messages = [LlamaChatMessage::new(
            "user".to_string(),
            prompt.to_string(),
        )?];
        Ok(model.apply_chat_template(&template, &messages, true)?)
    };
    chat_prompt().unwrap_or_else(|_| {
        format!("<|im_start|>user\n{prompt}<|im_end|>\n<|im_start|>assistant\n")
    })
}

fn local_context_tokens() -> u32 {
    std::env::var("DUKEMEMORY_LOCAL_GEN_CONTEXT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value >= 512)
        .unwrap_or(DEFAULT_CONTEXT_TOKENS)
}

fn local_max_new_tokens() -> usize {
    std::env::var("DUKEMEMORY_LOCAL_GEN_MAX_TOKENS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_NEW_TOKENS)
}

fn local_thread_count() -> i32 {
    std::env::var("DUKEMEMORY_LOCAL_GEN_THREADS")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|threads| threads.get().min(6) as i32)
                .unwrap_or(4)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hf_model_spec_supports_an_explicit_revision() -> Result<()> {
        let (repo, file, revision) =
            parse_hf_model_spec("hf://owner/model@immutable-commit:model.gguf")?;
        assert_eq!(repo, "owner/model");
        assert_eq!(file, "model.gguf");
        assert_eq!(revision, "immutable-commit");
        Ok(())
    }
}

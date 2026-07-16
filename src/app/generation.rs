use super::*;
use std::cell::RefCell;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const DEFAULT_MODEL_MAX_CONCURRENT: usize = 2;
const DEFAULT_MODEL_MAX_PROMPT_BYTES: usize = 262_144;
const DEFAULT_MODEL_MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_MODEL_MAX_OUTPUT_TOKENS: usize = 2_048;
const GENERATION_POLL_INTERVAL: Duration = Duration::from_millis(50);

thread_local! {
    static GENERATION_CANCELLATION: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

#[derive(Debug)]
struct GenerationLimiter {
    maximum: usize,
    active: Mutex<usize>,
    changed: Condvar,
}

#[derive(Debug)]
struct GenerationPermit {
    limiter: &'static GenerationLimiter,
}

impl Drop for GenerationPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.limiter.active.lock() {
            *active = active.saturating_sub(1);
            self.limiter.changed.notify_one();
        }
    }
}

impl GenerationLimiter {
    fn acquire(
        &'static self,
        cancellation: Option<&Arc<AtomicBool>>,
        deadline: Instant,
    ) -> Result<GenerationPermit> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("generation concurrency limiter lock was poisoned"))?;
        loop {
            ensure_generation_not_cancelled(cancellation)?;
            let now = Instant::now();
            if now >= deadline {
                bail!("generation concurrency queue timed out");
            }
            if *active < self.maximum {
                *active += 1;
                return Ok(GenerationPermit { limiter: self });
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(GENERATION_POLL_INTERVAL);
            let (next, _) = self
                .changed
                .wait_timeout(active, wait)
                .map_err(|_| anyhow::anyhow!("generation concurrency limiter lock was poisoned"))?;
            active = next;
        }
    }
}

static GENERATION_LIMITER: OnceLock<GenerationLimiter> = OnceLock::new();

struct GenerationCancellationScope {
    previous: Option<Arc<AtomicBool>>,
}

impl Drop for GenerationCancellationScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        GENERATION_CANCELLATION.with(|slot| {
            slot.replace(previous);
        });
    }
}

pub(crate) fn with_generation_cancellation<T>(
    cancellation: Arc<AtomicBool>,
    operation: impl FnOnce() -> T,
) -> T {
    let previous = GENERATION_CANCELLATION.with(|slot| slot.replace(Some(cancellation)));
    let _scope = GenerationCancellationScope { previous };
    operation()
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaChatMessage<'a>>,
    stream: bool,
    options: OllamaChatOptions,
}

#[derive(Debug, Serialize)]
struct OllamaChatOptions {
    num_predict: usize,
}

#[derive(Debug, Serialize)]
struct OllamaChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponseMessage {
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiChatMessage<'a>>,
    max_tokens: usize,
}

#[derive(Debug, Serialize)]
struct OpenAiChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    message: OpenAiChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponseMessage {
    content: String,
}

pub(crate) fn generate_answer(
    provider: &str,
    endpoint: &str,
    model: &str,
    prompt: &str,
) -> Result<String> {
    ensure_prompt_within_limit(prompt, model_max_prompt_bytes())?;
    let provider = provider.trim().to_lowercase();
    if provider == "mock" {
        return Ok(format!("Mock response for: {}", truncate_chars(prompt, 50)));
    }
    validate_generation_provider(&provider)?;

    let cancellation = current_generation_cancellation();
    ensure_generation_not_cancelled(cancellation.as_ref())?;
    let timeout = Duration::from_secs(model_request_timeout_secs());
    let deadline = Instant::now() + timeout;
    let limiter = GENERATION_LIMITER.get_or_init(|| GenerationLimiter {
        maximum: bounded_env_usize(
            "DUKEMEMORY_MODEL_MAX_CONCURRENT",
            DEFAULT_MODEL_MAX_CONCURRENT,
            1,
            64,
        ),
        active: Mutex::new(0),
        changed: Condvar::new(),
    });
    let permit = limiter.acquire(cancellation.as_ref(), deadline)?;
    let request_timeout = deadline.saturating_duration_since(Instant::now());
    if request_timeout.is_zero() {
        bail!("generation request timed out before worker start");
    }
    let endpoint = endpoint.to_string();
    let model = model.to_string();
    let prompt = prompt.to_string();
    let worker = std::thread::Builder::new()
        .name("dukememory-generation".to_string())
        .spawn(move || {
            let _permit = permit;
            match provider.as_str() {
                "ollama" => fetch_ollama_completion(&endpoint, &model, &prompt, request_timeout),
                "openai" | "openai-compatible" | "openai_compatible" => {
                    fetch_openai_completion(&endpoint, &model, &prompt, request_timeout)
                }
                "local" | "local-llama" | "local_llama" | "llama-cpp" | "llama_cpp" => {
                    generate_local_completion(&endpoint, &model, &prompt)
                }
                other => unreachable!("validated generation provider: {other}"),
            }
        })
        .context("failed to start bounded generation worker")?;

    while Instant::now() < deadline {
        ensure_generation_not_cancelled(cancellation.as_ref())?;
        if worker.is_finished() {
            return worker
                .join()
                .map_err(|_| anyhow::anyhow!("generation worker panicked"))?;
        }
        std::thread::sleep(GENERATION_POLL_INTERVAL);
    }
    if worker.is_finished() {
        return worker
            .join()
            .map_err(|_| anyhow::anyhow!("generation worker panicked"))?;
    }
    bail!("generation request timed out")
}

fn validate_generation_provider(provider: &str) -> Result<()> {
    if matches!(
        provider,
        "ollama"
            | "openai"
            | "openai-compatible"
            | "openai_compatible"
            | "local"
            | "local-llama"
            | "local_llama"
            | "llama-cpp"
            | "llama_cpp"
    ) {
        Ok(())
    } else {
        bail!("unsupported generation provider: {provider}")
    }
}

#[cfg(feature = "local-generation")]
fn generate_local_completion(endpoint: &str, model: &str, prompt: &str) -> Result<String> {
    crate::app::local_generation::generate_local(endpoint, model, prompt)
}

#[cfg(not(feature = "local-generation"))]
fn generate_local_completion(_endpoint: &str, _model: &str, _prompt: &str) -> Result<String> {
    bail!("local generation provider requires building dukememory with --features local-generation")
}

fn fetch_ollama_completion(
    endpoint: &str,
    model: &str,
    prompt: &str,
    timeout: Duration,
) -> Result<String> {
    let url = format!("{}/api/chat", endpoint.trim_end_matches('/'));
    let (client, url) = egress::blocking_http_client(&url, timeout)?;
    let messages = vec![OllamaChatMessage {
        role: "user",
        content: prompt,
    }];
    let response = client
        .post(url)
        .json(&OllamaChatRequest {
            model,
            messages,
            stream: false,
            options: OllamaChatOptions {
                num_predict: model_max_output_tokens(),
            },
        })
        .send()?
        .error_for_status()?;
    let response: OllamaChatResponse = read_bounded_json(response)?;
    Ok(response.message.content)
}

fn fetch_openai_completion(
    endpoint: &str,
    model: &str,
    prompt: &str,
    timeout: Duration,
) -> Result<String> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let (client, url) = egress::blocking_http_client(&url, timeout)?;
    let messages = vec![OpenAiChatMessage {
        role: "user",
        content: prompt,
    }];
    let mut request = client.post(url).json(&OpenAiChatRequest {
        model,
        messages,
        max_tokens: model_max_output_tokens(),
    });
    if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
        && !key.trim().is_empty()
    {
        request = request.bearer_auth(key);
    }
    let response = request.send()?.error_for_status()?;
    let response: OpenAiChatResponse = read_bounded_json(response)?;
    let content = response
        .choices
        .into_iter()
        .next()
        .map(|item| item.message.content)
        .unwrap_or_default();
    Ok(content)
}

fn model_request_timeout_secs() -> u64 {
    std::env::var("DUKEMEMORY_MODEL_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.clamp(1, 300))
        .unwrap_or(60)
}

fn model_max_prompt_bytes() -> usize {
    bounded_env_usize(
        "DUKEMEMORY_MODEL_MAX_PROMPT_BYTES",
        DEFAULT_MODEL_MAX_PROMPT_BYTES,
        4_096,
        16 * 1024 * 1024,
    )
}

fn model_max_response_bytes() -> usize {
    bounded_env_usize(
        "DUKEMEMORY_MODEL_MAX_RESPONSE_BYTES",
        DEFAULT_MODEL_MAX_RESPONSE_BYTES,
        16_384,
        32 * 1024 * 1024,
    )
}

fn model_max_output_tokens() -> usize {
    bounded_env_usize(
        "DUKEMEMORY_MODEL_MAX_OUTPUT_TOKENS",
        DEFAULT_MODEL_MAX_OUTPUT_TOKENS,
        1,
        32_768,
    )
}

fn bounded_env_usize(name: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(minimum, maximum))
        .unwrap_or(default)
}

fn ensure_prompt_within_limit(prompt: &str, maximum: usize) -> Result<()> {
    if prompt.len() > maximum {
        bail!(
            "generation prompt exceeds {maximum} bytes; reduce the RAG budget or DUKEMEMORY_MODEL_MAX_PROMPT_BYTES"
        );
    }
    Ok(())
}

fn read_bounded_json<T: serde::de::DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T> {
    let maximum = model_max_response_bytes();
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        bail!("generation response exceeds {maximum} bytes");
    }
    let mut body = Vec::with_capacity(maximum.min(64 * 1024));
    response
        .take(maximum.saturating_add(1) as u64)
        .read_to_end(&mut body)?;
    if body.len() > maximum {
        bail!("generation response exceeds {maximum} bytes");
    }
    serde_json::from_slice(&body).context("invalid bounded generation response JSON")
}

fn current_generation_cancellation() -> Option<Arc<AtomicBool>> {
    GENERATION_CANCELLATION.with(|slot| slot.borrow().clone())
}

fn ensure_generation_not_cancelled(cancellation: Option<&Arc<AtomicBool>>) -> Result<()> {
    if cancellation.is_some_and(|value| value.load(Ordering::Acquire)) {
        bail!("generation request was cancelled");
    }
    Ok(())
}

pub(crate) fn generate_tour_narrative(
    config: &crate::runtime_config::GenerationConfig,
    topology: &crate::app::topology::TopologyResult,
) -> Result<String> {
    let mut prompt = String::new();
    prompt.push_str("You are an expert technical educator designing a guided tour of a codebase's semantic memory.\n");
    prompt.push_str("Below is the structural topology of the project's knowledge graph:\n\n");
    prompt.push_str(&format!(
        "Graph summary: {} nodes, {} edges.\n\n",
        topology.total_nodes, topology.total_edges
    ));

    prompt.push_str("Top Entry Points (where users should start):\n");
    for ep in &topology.entry_points {
        if let Some(node) = topology.nodes.get(ep) {
            prompt.push_str(&format!(
                "- [{}] {}: {}\n",
                node.memory_type,
                node.title,
                node.body.lines().next().unwrap_or("")
            ));
        } else {
            prompt.push_str(&format!("- {}\n", ep));
        }
    }

    prompt.push_str("\nHigh Fan-In Nodes (many references):\n");
    for (id, count) in topology.fan_in.iter().take(8) {
        prompt.push_str(&format!("- {} incoming links: {}\n", count, id));
    }

    prompt.push_str("\nHigh Fan-Out Nodes (many outgoing links):\n");
    for (id, count) in topology.fan_out.iter().take(8) {
        prompt.push_str(&format!("- {} outgoing links: {}\n", count, id));
    }

    prompt.push_str("\nNodes by Depth (Reading Order):\n");
    let mut depths: Vec<_> = topology.bfs_by_depth.keys().copied().collect();
    depths.sort();
    for d in depths {
        prompt.push_str(&format!("Depth {}:\n", d));
        if let Some(nodes) = topology.bfs_by_depth.get(&d) {
            for n in nodes {
                if let Some(node) = topology.nodes.get(n) {
                    prompt.push_str(&format!(
                        "  - [{}] {}: {}\n",
                        node.memory_type,
                        node.title,
                        node.body.lines().next().unwrap_or("")
                    ));
                } else {
                    prompt.push_str(&format!("  - {}\n", n));
                }
            }
        }
    }

    prompt.push_str("\nTask: Create a 5-10 step Guided Tour. Each step should explain what the node/concept is and why it's important. Provide the result in clear Markdown format, ordered by Step 1, Step 2, etc. Use the provided topological ordering.");

    generate_answer(&config.provider, &config.endpoint, &config.model, &prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_prompt_budget_fails_closed() {
        let error = ensure_prompt_within_limit("oversized", 4)
            .unwrap_err()
            .to_string();
        assert!(error.contains("generation prompt exceeds 4 bytes"));
    }

    #[test]
    fn generation_cancellation_prevents_network_start() {
        let cancellation = Arc::new(AtomicBool::new(true));
        let error = with_generation_cancellation(cancellation, || {
            generate_answer(
                "ollama",
                "http://localhost:9",
                "fixture",
                "cancel before network",
            )
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("cancelled"));
    }

    #[test]
    fn expired_generation_queue_deadline_never_grants_available_capacity() {
        let limiter = Box::leak(Box::new(GenerationLimiter {
            maximum: 1,
            active: Mutex::new(0),
            changed: Condvar::new(),
        }));
        let error = limiter
            .acquire(None, Instant::now() - Duration::from_millis(1))
            .unwrap_err()
            .to_string();
        assert!(error.contains("queue timed out"));
        assert_eq!(*limiter.active.lock().unwrap(), 0);
    }

    #[test]
    fn unsupported_generation_provider_fails_before_queue_or_network() {
        let error = generate_answer("unsupported", "http://localhost:9", "fixture", "prompt")
            .unwrap_err()
            .to_string();
        assert_eq!(error, "unsupported generation provider: unsupported");
    }

    #[test]
    fn provider_payloads_include_output_token_limits() {
        let ollama = serde_json::to_value(OllamaChatRequest {
            model: "fixture",
            messages: vec![],
            stream: false,
            options: OllamaChatOptions { num_predict: 42 },
        })
        .unwrap();
        let openai = serde_json::to_value(OpenAiChatRequest {
            model: "fixture",
            messages: vec![],
            max_tokens: 42,
        })
        .unwrap();
        assert_eq!(ollama["options"]["num_predict"], 42);
        assert_eq!(openai["max_tokens"], 42);
    }
}

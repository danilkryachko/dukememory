use super::*;

#[derive(Debug, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaChatMessage<'a>>,
    stream: bool,
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
    match provider.trim().to_lowercase().as_str() {
        "ollama" => fetch_ollama_completion(endpoint, model, prompt),
        "openai" | "openai-compatible" | "openai_compatible" => {
            fetch_openai_completion(endpoint, model, prompt)
        }
        "mock" => Ok(format!("Mock response for: {}", truncate_chars(prompt, 50))),
        other => bail!("unsupported generation provider: {other}"),
    }
}

fn fetch_ollama_completion(endpoint: &str, model: &str, prompt: &str) -> Result<String> {
    let url = format!("{}/api/chat", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
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
        })
        .send()?
        .error_for_status()?
        .json::<OllamaChatResponse>()?;
    Ok(response.message.content)
}

fn fetch_openai_completion(endpoint: &str, model: &str, prompt: &str) -> Result<String> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let messages = vec![OpenAiChatMessage {
        role: "user",
        content: prompt,
    }];
    let mut request = client
        .post(url)
        .json(&OpenAiChatRequest { model, messages });
    if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
        && !key.trim().is_empty()
    {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()?
        .error_for_status()?
        .json::<OpenAiChatResponse>()?;
    let content = response
        .choices
        .into_iter()
        .next()
        .map(|item| item.message.content)
        .unwrap_or_default();
    Ok(content)
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

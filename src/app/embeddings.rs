use super::*;
use sha2::{Digest, Sha256};

pub(crate) fn embed_index(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    statuses: &[String],
    limit: Option<usize>,
    force: bool,
) -> Result<EmbeddingIndexReport> {
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let statuses = if statuses.is_empty() {
        vec!["active".to_string(), "uncertain".to_string()]
    } else {
        statuses.to_vec()
    };
    let rows = query_memories(
        conn,
        None,
        &[],
        &statuses,
        None,
        limit.unwrap_or(usize::MAX),
    )?;
    let mut indexed = 0;
    let mut skipped = 0;
    for memory in rows {
        let content = embedding_content(&memory);
        let hash = content_hash(&content);
        if !force && embedding_is_current(conn, &memory.id, &endpoint_key, model, &hash)? {
            skipped += 1;
            continue;
        }
        let embedding = fetch_embedding(provider, endpoint, model, &content)
            .with_context(|| format!("embedding failed for memory {}", memory.id))?;
        store_embedding(conn, &memory.id, &endpoint_key, model, &hash, &embedding)?;
        indexed += 1;
    }
    Ok(EmbeddingIndexReport {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        indexed,
        skipped,
    })
}

pub(crate) fn semantic_search(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<EmbeddingRow>> {
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let query_embedding = fetch_embedding(provider, endpoint, model, query)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT memory_id, embedding
        FROM memory_embeddings
        WHERE endpoint = ?1 AND model = ?2
        "#,
    )?;
    let rows = stmt.query_map(params![endpoint_key, model], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut scored = Vec::new();
    for row in rows {
        let (memory_id, raw_embedding) = row?;
        let embedding: Vec<f32> = serde_json::from_str(&raw_embedding)?;
        let score = cosine_similarity(&query_embedding, &embedding);
        let memory = get_memory_with_links(conn, &memory_id)?;
        scored.push(EmbeddingRow { memory, score });
    }
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);
    Ok(scored)
}

pub(crate) fn semantic_index_ready(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<bool> {
    let report = embed_status(conn, provider, endpoint, model)?;
    Ok(report.indexed > report.stale)
}

fn embedding_endpoint_key(provider: &str, endpoint: &str) -> String {
    format!("{}:{}", provider.trim().to_lowercase(), endpoint.trim())
}

fn embedding_content(memory: &Memory) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        memory.memory_type, memory.scope, memory.title, memory.body, memory.status
    )
}

pub(crate) fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn embedding_is_current(
    conn: &Connection,
    memory_id: &str,
    endpoint: &str,
    model: &str,
    hash: &str,
) -> Result<bool> {
    let existing: Option<String> = conn
        .query_row(
            r#"
            SELECT content_hash FROM memory_embeddings
            WHERE memory_id = ?1 AND endpoint = ?2 AND model = ?3
            "#,
            params![memory_id, endpoint, model],
            |row| row.get(0),
        )
        .optional()?;
    Ok(existing.as_deref() == Some(hash))
}

fn store_embedding(
    conn: &Connection,
    memory_id: &str,
    endpoint: &str,
    model: &str,
    hash: &str,
    embedding: &[f32],
) -> Result<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO memory_embeddings (
            memory_id, model, endpoint, dimensions, embedding, content_hash, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            memory_id,
            model,
            endpoint,
            embedding.len() as i64,
            serde_json::to_string(embedding)?,
            hash,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn fetch_ollama_embedding(endpoint: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    let url = format!("{}/api/embeddings", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let response = client
        .post(url)
        .json(&OllamaEmbeddingRequest {
            model,
            prompt: text,
        })
        .send()?
        .error_for_status()?
        .json::<OllamaEmbeddingResponse>()?;
    if response.embedding.is_empty() {
        bail!("embedding response was empty");
    }
    Ok(response.embedding)
}

#[derive(Debug, Serialize)]
struct OpenAiEmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

fn fetch_openai_embedding(endpoint: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    let url = format!("{}/v1/embeddings", endpoint.trim_end_matches('/'));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    let mut request = client
        .post(url)
        .json(&OpenAiEmbeddingRequest { model, input: text });
    if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
        && !key.trim().is_empty()
    {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()?
        .error_for_status()?
        .json::<OpenAiEmbeddingResponse>()?;
    let embedding = response
        .data
        .into_iter()
        .next()
        .map(|item| item.embedding)
        .unwrap_or_default();
    if embedding.is_empty() {
        bail!("embedding response was empty");
    }
    Ok(embedding)
}

fn fetch_mock_embedding(model: &str, text: &str) -> Vec<f32> {
    let dims = if model.contains("small") { 64 } else { 128 };
    let mut values = vec![0.0f32; dims];
    for token in tokenize(text) {
        let mut hasher = Sha256::new();
        hasher.update(model.as_bytes());
        hasher.update(token.as_bytes());
        let hash = hasher.finalize();
        for (i, byte) in hash.iter().enumerate() {
            let idx = ((i * 31) + (*byte as usize)) % dims;
            values[idx] += ((*byte as f32) / 127.5) - 1.0;
        }
    }
    if values.iter().all(|value| *value == 0.0) {
        values[0] = 1.0;
    }
    values
}

fn fetch_embedding(provider: &str, endpoint: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    match provider.trim().to_lowercase().as_str() {
        "ollama" => fetch_ollama_embedding(endpoint, model, text),
        "openai" | "openai-compatible" | "openai_compatible" => {
            fetch_openai_embedding(endpoint, model, text)
        }
        "mock" => Ok(fetch_mock_embedding(model, text)),
        other => bail!("unsupported embedding provider: {other}"),
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for i in 0..len {
        let av = a[i] as f64;
        let bv = b[i] as f64;
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}

pub(crate) fn print_provider_models(provider: &str, endpoint: &str, json_out: bool) -> Result<()> {
    let models = provider_models(provider, endpoint)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&models)?);
    } else if models.is_empty() {
        println!("models: none");
    } else {
        for model in models {
            println!("{}", model.name);
        }
    }
    Ok(())
}

fn provider_models(provider: &str, endpoint: &str) -> Result<Vec<ProviderModel>> {
    match provider.trim().to_lowercase().as_str() {
        "mock" => Ok(vec![ProviderModel {
            name: "mock-embedding".to_string(),
            details: None,
        }]),
        "ollama" => {
            let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
            let value: Value = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?
                .get(url)
                .send()?
                .error_for_status()?
                .json()?;
            let models = value
                .get("models")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .map(|name| ProviderModel {
                            name: name.to_string(),
                            details: Some(item.clone()),
                        })
                })
                .collect();
            Ok(models)
        }
        "openai" | "openai-compatible" | "openai_compatible" => {
            let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
            let mut request = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?
                .get(url);
            if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
                && !key.trim().is_empty()
            {
                request = request.bearer_auth(key);
            }
            let value: Value = request.send()?.error_for_status()?.json()?;
            let models = value
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .map(|name| ProviderModel {
                            name: name.to_string(),
                            details: Some(item.clone()),
                        })
                })
                .collect();
            Ok(models)
        }
        other => bail!("unsupported embedding provider: {other}"),
    }
}

pub(crate) fn print_vector_bench(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<()> {
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let mut stmt = conn.prepare(
        r#"
        SELECT embedding
        FROM memory_embeddings
        WHERE endpoint = ?1 AND model = ?2
        "#,
    )?;
    let embeddings = stmt
        .query_map(params![endpoint_key, model], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|raw| serde_json::from_str::<Vec<f32>>(&raw).map_err(Into::into))
        .collect::<Result<Vec<_>>>()?;
    if embeddings.is_empty() {
        println!("vectors: 0");
        println!("bench: no indexed embeddings");
        return Ok(());
    }
    let query = embeddings[0].clone();
    let started = std::time::Instant::now();
    let mut best = f64::NEG_INFINITY;
    for embedding in &embeddings {
        best = best.max(cosine_similarity(&query, embedding));
    }
    let elapsed = started.elapsed();
    println!("vectors: {}", embeddings.len());
    println!("dimensions: {}", query.len());
    println!("best_score: {best:.4}");
    println!("elapsed_ms: {:.3}", elapsed.as_secs_f64() * 1000.0);
    Ok(())
}

#[derive(Debug, Serialize)]
pub(crate) struct EmbedStatusReport {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) eligible: usize,
    pub(crate) indexed: usize,
    pub(crate) stale: usize,
    pub(crate) missing: usize,
    pub(crate) provider_reachable: bool,
    pub(crate) provider_health_ms: Option<u128>,
    pub(crate) provider_error: Option<String>,
}

pub(crate) fn print_embed_status(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    json_out: bool,
) -> Result<()> {
    let report = embed_status(conn, provider, endpoint, model)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("provider: {}", report.provider);
        println!("endpoint: {}", report.endpoint);
        println!("model: {}", report.model);
        println!("eligible: {}", report.eligible);
        println!("indexed: {}", report.indexed);
        println!("missing: {}", report.missing);
        println!("stale: {}", report.stale);
        println!("provider_reachable: {}", report.provider_reachable);
        println!(
            "provider_health_ms: {}",
            report
                .provider_health_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
        if let Some(error) = &report.provider_error {
            println!("provider_error: {error}");
        }
    }
    Ok(())
}

pub(crate) fn embed_status(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<EmbedStatusReport> {
    let endpoint_key = embedding_endpoint_key(provider, endpoint);
    let rows = query_memories(
        conn,
        None,
        &[],
        &["active".to_string(), "uncertain".to_string()],
        None,
        usize::MAX,
    )?;
    let mut indexed = 0;
    let mut stale = 0;
    let mut missing = 0;
    for memory in &rows {
        let content = embedding_content(memory);
        let hash = content_hash(&content);
        let existing: Option<String> = conn
            .query_row(
                r#"
                SELECT content_hash FROM memory_embeddings
                WHERE memory_id = ?1 AND endpoint = ?2 AND model = ?3
                "#,
                params![memory.id, endpoint_key, model],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(existing_hash) if existing_hash == hash => indexed += 1,
            Some(_) => {
                indexed += 1;
                stale += 1;
            }
            None => missing += 1,
        }
    }
    let provider_health = embedding_provider_health(provider, endpoint);
    Ok(EmbedStatusReport {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        eligible: rows.len(),
        indexed,
        stale,
        missing,
        provider_reachable: provider_health.reachable,
        provider_health_ms: provider_health.elapsed_ms,
        provider_error: provider_health.error,
    })
}

struct EmbeddingProviderHealth {
    reachable: bool,
    elapsed_ms: Option<u128>,
    error: Option<String>,
}

fn embedding_provider_health(provider: &str, endpoint: &str) -> EmbeddingProviderHealth {
    let started = std::time::Instant::now();
    let result = match provider.trim().to_lowercase().as_str() {
        "mock" => Ok(()),
        "ollama" => {
            let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_millis(1500))
                .build()
                .and_then(|client| client.get(url).send())
                .and_then(|response| response.error_for_status().map(|_| ()))
                .map_err(Into::into)
        }
        "openai" | "openai-compatible" | "openai_compatible" => {
            let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
            let client = match reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_millis(1500))
                .build()
            {
                Ok(client) => client,
                Err(error) => return provider_health_error(started, error.into()),
            };
            let mut request = client.get(url);
            if let Ok(key) = std::env::var("DUKEMEMORY_OPENAI_API_KEY")
                && !key.trim().is_empty()
            {
                request = request.bearer_auth(key);
            }
            request
                .send()
                .and_then(|response| response.error_for_status().map(|_| ()))
                .map_err(Into::into)
        }
        other => Err(anyhow::anyhow!("unsupported embedding provider: {other}")),
    };
    match result {
        Ok(()) => EmbeddingProviderHealth {
            reachable: true,
            elapsed_ms: Some(started.elapsed().as_millis()),
            error: None,
        },
        Err(error) => provider_health_error(started, error),
    }
}

fn provider_health_error(
    started: std::time::Instant,
    error: anyhow::Error,
) -> EmbeddingProviderHealth {
    EmbeddingProviderHealth {
        reachable: false,
        elapsed_ms: Some(started.elapsed().as_millis()),
        error: Some(truncate_chars(&error.to_string(), 220)),
    }
}

pub(crate) fn embed_watch(
    conn: &Connection,
    provider: &str,
    endpoint: &str,
    model: &str,
    interval_secs: u64,
    once: bool,
) -> Result<()> {
    loop {
        let report = embed_index(conn, provider, endpoint, model, &[], None, false)?;
        println!(
            "indexed={} skipped={} provider={} model={}",
            report.indexed, report.skipped, report.provider, report.model
        );
        if once {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(1)));
    }
}

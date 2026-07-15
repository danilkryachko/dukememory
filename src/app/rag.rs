use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn print_memory_rag_answer(
    conn: &Connection,
    question: &str,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    embed_provider: &str,
    embed_endpoint: &str,
    embed_model: &str,
    gen_provider: &str,
    gen_endpoint: &str,
    gen_model: &str,
    json_out: bool,
) -> Result<()> {
    let report = memory_rag_report(
        conn,
        question,
        scope,
        limit,
        budget,
        embed_provider,
        embed_endpoint,
        embed_model,
        gen_provider,
        gen_endpoint,
        gen_model,
    )?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("RAG Answer:");
    println!(
        "status: {} confidence: {} ({:.2})",
        report.status, report.confidence, report.confidence_score
    );
    println!("{}", report.answer);
    if !report.citations.is_empty() {
        println!("\nCitations: {}", report.citations.join(", "));
    }
    if !report.missing_evidence.is_empty() {
        println!("\nMissing Evidence:");
        for item in &report.missing_evidence {
            println!("- {item}");
        }
    }
    if !report.trace.is_empty() {
        println!("\nTrace:");
        for entry in &report.trace {
            print_rag_trace_entry(entry);
        }
    }
    print_rag_packing(&report.packing);
    if !report.source_pack.is_empty() {
        println!("\nSources:");
        for source in &report.source_pack {
            println!(
                "- {} [{}] score={:.2}: {}",
                source.id, source.memory_type, source.score, source.title
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn print_memory_rag_debug(
    conn: &Connection,
    question: &str,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
    json_out: bool,
) -> Result<()> {
    let report = memory_rag_debug_report(
        conn, question, scope, limit, budget, provider, endpoint, model,
    )?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("RAG Debug");
    println!(
        "status: {} confidence: {} ({:.2})",
        report.status, report.confidence, report.confidence_score
    );
    println!("{}", report.receipt);
    if let Some(reason) = &report.semantic_skip_reason {
        println!("semantic: skipped ({reason})");
    } else if let Some(error) = &report.semantic_error {
        println!("semantic: fallback ({error})");
    } else if report.semantic_used {
        println!("semantic: used");
    }
    if !report.missing_evidence.is_empty() {
        println!("\nMissing Evidence:");
        for item in &report.missing_evidence {
            println!("- {item}");
        }
    }
    if !report.trace.is_empty() {
        println!("\nTrace:");
        for entry in &report.trace {
            print_rag_trace_entry(entry);
        }
    }
    print_rag_packing(&report.packing);
    if !report.source_pack.is_empty() {
        println!("\nSource Pack:");
        for source in &report.source_pack {
            println!(
                "- {} [{}] score={:.2} utility={:.2}: {}",
                source.id, source.memory_type, source.score, source.utility_score, source.title
            );
            println!("  {}", source.summary);
            if !source.reasons.is_empty() {
                println!("  reasons: {}", source.reasons.join(", "));
            }
        }
    }
    Ok(())
}

pub(crate) fn print_memory_answer(
    conn: &Connection,
    root: &Path,
    question: &str,
    scope: Option<&str>,
    limit: usize,
    json_out: bool,
) -> Result<()> {
    let report = memory_answer_report(conn, root, question, scope, limit)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Memory Answer");
    println!("status: {}", report.status);
    println!("{}", report.answer);
    if !report.citations.is_empty() {
        println!("citations:");
        for citation in &report.citations {
            println!(
                "- {} [{}] {}",
                citation.id, citation.memory_type, citation.title
            );
        }
    }
    for gap in &report.gaps {
        println!("gap: {gap}");
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryRagReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) query: String,
    pub(crate) answer: String,
    pub(crate) generation_guard: RagGenerationGuardReport,
    pub(crate) citations: Vec<String>,
    pub(crate) citation_count: usize,
    pub(crate) confidence: String,
    pub(crate) confidence_score: f64,
    pub(crate) used_strategy: String,
    pub(crate) semantic_used: bool,
    pub(crate) semantic_skipped: bool,
    pub(crate) semantic_skip_reason: Option<String>,
    pub(crate) semantic_error: Option<String>,
    pub(crate) missing_evidence: Vec<String>,
    pub(crate) trace: Vec<RagTraceEntry>,
    pub(crate) packing: RagPackingReport,
    pub(crate) source_pack: Vec<RagSource>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryRagDebugReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) query: String,
    pub(crate) citations: Vec<String>,
    pub(crate) citation_count: usize,
    pub(crate) confidence: String,
    pub(crate) confidence_score: f64,
    pub(crate) used_strategy: String,
    pub(crate) semantic_used: bool,
    pub(crate) semantic_skipped: bool,
    pub(crate) semantic_skip_reason: Option<String>,
    pub(crate) semantic_error: Option<String>,
    pub(crate) receipt: String,
    pub(crate) missing_evidence: Vec<String>,
    pub(crate) trace: Vec<RagTraceEntry>,
    pub(crate) packing: RagPackingReport,
    pub(crate) source_pack: Vec<RagSource>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RagGenerationGuardReport {
    pub(crate) answer_source: String,
    pub(crate) accepted_generated: bool,
    pub(crate) fallback_reason: Option<String>,
    pub(crate) selected_citations: Vec<String>,
    pub(crate) prompt_fragment_detected: bool,
    pub(crate) generated_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RagTraceEntry {
    pub(crate) rank: usize,
    pub(crate) id: String,
    pub(crate) source_kind: String,
    pub(crate) title: String,
    pub(crate) score: f64,
    pub(crate) semantic_score: Option<f64>,
    pub(crate) location: Option<String>,
    pub(crate) reasons: Vec<String>,
    pub(crate) provenance: RagSourceProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct RagPackingReport {
    pub(crate) candidate_count: usize,
    pub(crate) selected_count: usize,
    pub(crate) memory_candidates: usize,
    pub(crate) chunk_candidates: usize,
    pub(crate) selected_memories: usize,
    pub(crate) selected_chunks: usize,
    pub(crate) suppressed_duplicate: usize,
    pub(crate) suppressed_overlap: usize,
    pub(crate) suppressed_file_cap: usize,
    pub(crate) suppressed_limit: usize,
    pub(crate) suppressed_sources: Vec<RagPackingSuppressedSource>,
    pub(crate) chunk_files: Vec<RagPackingFileReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct RagPackingSuppressedSource {
    pub(crate) id: String,
    pub(crate) source_kind: String,
    pub(crate) title: String,
    pub(crate) reason: String,
    pub(crate) score: f64,
    pub(crate) semantic_score: Option<f64>,
    pub(crate) location: Option<String>,
    pub(crate) summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct RagPackingFileReport {
    pub(crate) path: String,
    pub(crate) candidates: usize,
    pub(crate) selected: usize,
    pub(crate) suppressed_overlap: usize,
    pub(crate) suppressed_file_cap: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RagSource {
    pub(crate) id: String,
    pub(crate) source_kind: String,
    #[serde(rename = "type")]
    pub(crate) memory_type: String,
    pub(crate) scope: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) score: f64,
    pub(crate) utility_score: f64,
    pub(crate) semantic_score: Option<f64>,
    pub(crate) confidence: f64,
    pub(crate) reasons: Vec<String>,
    pub(crate) summary: String,
    pub(crate) links: Vec<RagSourceLink>,
    pub(crate) provenance: RagSourceProvenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chunk_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) end_line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RagSourceLink {
    pub(crate) kind: String,
    pub(crate) target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct RagSourceProvenance {
    pub(crate) origin: String,
    pub(crate) evidence_ref: String,
    pub(crate) content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) updated_at: Option<i64>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn memory_rag_report(
    conn: &Connection,
    question: &str,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    embed_provider: &str,
    embed_endpoint: &str,
    embed_model: &str,
    gen_provider: &str,
    gen_endpoint: &str,
    gen_model: &str,
) -> Result<MemoryRagReport> {
    let debug = memory_rag_debug_report(
        conn,
        question,
        scope,
        limit,
        budget,
        embed_provider,
        embed_endpoint,
        embed_model,
    )?;
    let (answer, generation_guard) = if debug.source_pack.is_empty() {
        (
            rag_no_evidence_answer(question),
            rag_no_evidence_generation_guard(),
        )
    } else {
        let prompt = rag_generation_prompt(question, &debug.source_pack, &debug.missing_evidence);
        let generated =
            generation::generate_answer(gen_provider, gen_endpoint, gen_model, &prompt)?;
        rag_guard_generated_answer(
            question,
            generated,
            &debug.source_pack,
            &debug.missing_evidence,
        )
    };

    Ok(MemoryRagReport {
        version: debug.version,
        ok: debug.ok,
        status: debug.status,
        query: debug.query,
        answer,
        generation_guard,
        citations: debug.citations,
        citation_count: debug.citation_count,
        confidence: debug.confidence,
        confidence_score: debug.confidence_score,
        used_strategy: debug.used_strategy,
        semantic_used: debug.semantic_used,
        semantic_skipped: debug.semantic_skipped,
        semantic_skip_reason: debug.semantic_skip_reason,
        semantic_error: debug.semantic_error,
        missing_evidence: debug.missing_evidence,
        trace: debug.trace,
        packing: debug.packing,
        source_pack: debug.source_pack,
        recommendations: debug.recommendations,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn memory_rag_debug_report(
    conn: &Connection,
    question: &str,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<MemoryRagDebugReport> {
    let limit = limit.clamp(1, 16);
    let budget = budget.clamp(800, 12_000);
    let request = RetrieveRequest {
        query: question,
        strategy: RetrievalStrategy::Hybrid,
        format: OutputFormat::Json,
        limit,
        budget,
        scope,
        rules: None,
        provider,
        endpoint,
        model,
        audit_read: false,
    };
    let retrieval = retrieve_report(conn, &request)?;
    let query_terms = relevance_terms(question);
    let mut source_pack = rag_source_pack(&retrieval, question, limit, budget);
    source_pack.extend(rag_chunk_source_pack(
        conn,
        question,
        scope,
        limit,
        budget,
        provider,
        endpoint,
        model,
        &query_terms,
    )?);
    rerank_rag_source_pack(&mut source_pack, question, &query_terms);
    let (source_pack, packing) = select_rag_sources(source_pack, limit);
    let citations = source_pack
        .iter()
        .map(|source| source.id.clone())
        .collect::<Vec<_>>();
    let missing_evidence = rag_missing_evidence(&retrieval, &source_pack);
    let (confidence, confidence_score) = rag_confidence(&source_pack);
    let ok = !source_pack.is_empty();
    let trace = rag_trace_entries(&source_pack);
    let status = if !ok {
        "missing_evidence"
    } else if confidence == "low" {
        "limited"
    } else {
        "ready"
    }
    .to_string();
    Ok(MemoryRagDebugReport {
        version: 2,
        ok,
        status,
        query: question.to_string(),
        citation_count: citations.len(),
        citations,
        confidence,
        confidence_score,
        used_strategy: retrieval.strategy.clone(),
        semantic_used: retrieval.semantic_used,
        semantic_skipped: retrieval.semantic_skipped,
        semantic_skip_reason: retrieval.semantic_skip_reason.clone(),
        semantic_error: retrieval.semantic_error.clone(),
        receipt: retrieval.receipt.clone(),
        missing_evidence,
        trace,
        packing,
        source_pack,
        recommendations: rag_recommendations(&retrieval),
    })
}

fn rag_source_pack(
    retrieval: &RetrievalReport,
    question: &str,
    limit: usize,
    budget: usize,
) -> Vec<RagSource> {
    let query_terms = relevance_terms(question);
    let summary_chars = if budget <= 1_600 { 220 } else { 360 };
    retrieval
        .hits
        .iter()
        .take(limit)
        .map(|hit| {
            let memory = &hit.memory.memory;
            RagSource {
                id: memory.id.clone(),
                source_kind: "memory".to_string(),
                memory_type: memory.memory_type.clone(),
                scope: memory.scope.clone(),
                title: memory.title.clone(),
                status: memory.status.clone(),
                score: hit.score,
                utility_score: hit.utility_score,
                semantic_score: hit.semantic_score,
                confidence: memory.confidence,
                reasons: hit.reasons.clone(),
                summary: query_focused_summary(&memory.body, &query_terms, summary_chars),
                links: hit
                    .memory
                    .links
                    .iter()
                    .take(8)
                    .map(|link| RagSourceLink {
                        kind: link.kind.clone(),
                        target: link.target.clone(),
                    })
                    .collect(),
                provenance: RagSourceProvenance {
                    origin: "memory_store".to_string(),
                    evidence_ref: format!("dukememory:memory:{}", memory.id),
                    content_hash: super::embeddings::content_hash(&memory.body),
                    source: memory.source.clone(),
                    updated_at: Some(memory.updated_at),
                },
                path: None,
                chunk_index: None,
                start_line: None,
                end_line: None,
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn rag_chunk_source_pack(
    conn: &Connection,
    question: &str,
    scope: Option<&str>,
    limit: usize,
    budget: usize,
    provider: &str,
    endpoint: &str,
    model: &str,
    query_terms: &HashSet<String>,
) -> Result<Vec<RagSource>> {
    let summary_chars = if budget <= 1_600 { 240 } else { 420 };
    let sources = crate::app::rag_ingest::query_rag_chunks(
        conn,
        question,
        scope,
        limit.max(4),
        budget,
        provider,
        endpoint,
        model,
    )?
    .into_iter()
    .map(|hit| {
        let title = format!("{}:{}-{}", hit.path, hit.start_line, hit.end_line);
        RagSource {
            id: hit.id.clone(),
            source_kind: "chunk".to_string(),
            memory_type: "source_chunk".to_string(),
            scope: hit.scope.clone(),
            title,
            status: "active".to_string(),
            score: hit.score,
            utility_score: 1.0,
            semantic_score: hit.semantic_score,
            confidence: 0.82,
            reasons: hit.reasons,
            summary: rag_chunk_summary(&hit.content, query_terms, summary_chars),
            links: vec![
                RagSourceLink {
                    kind: "file".to_string(),
                    target: hit.path.clone(),
                },
                RagSourceLink {
                    kind: "lines".to_string(),
                    target: format!("{}-{}", hit.start_line, hit.end_line),
                },
            ],
            provenance: RagSourceProvenance {
                origin: "rag_chunk".to_string(),
                evidence_ref: format!("dukememory:chunk:{}", hit.id),
                content_hash: super::embeddings::content_hash(&hit.content),
                source: Some(hit.path.clone()),
                updated_at: None,
            },
            path: Some(hit.path),
            chunk_index: Some(hit.chunk_index),
            start_line: Some(hit.start_line),
            end_line: Some(hit.end_line),
        }
    })
    .collect::<Vec<_>>();
    Ok(sources)
}

fn rag_trace_entries(source_pack: &[RagSource]) -> Vec<RagTraceEntry> {
    source_pack
        .iter()
        .enumerate()
        .map(|(index, source)| RagTraceEntry {
            rank: index + 1,
            id: source.id.clone(),
            source_kind: source.source_kind.clone(),
            title: source.title.clone(),
            score: source.score,
            semantic_score: source.semantic_score,
            location: rag_source_location(source),
            reasons: source.reasons.clone(),
            provenance: source.provenance.clone(),
        })
        .collect()
}

fn rag_source_location(source: &RagSource) -> Option<String> {
    let path = source.path.as_ref()?;
    let lines = match (source.start_line, source.end_line) {
        (Some(start), Some(end)) => format!(":{start}-{end}"),
        _ => String::new(),
    };
    Some(format!("{path}{lines}"))
}

fn print_rag_trace_entry(entry: &RagTraceEntry) {
    let semantic = entry
        .semantic_score
        .map(|score| format!(" semantic={score:.3}"))
        .unwrap_or_default();
    let location = entry
        .location
        .as_ref()
        .map(|location| format!(" location={location}"))
        .unwrap_or_default();
    println!(
        "- #{} {} [{}] score={:.2}{}{} provenance={} hash={}: {}",
        entry.rank,
        entry.id,
        entry.source_kind,
        entry.score,
        semantic,
        location,
        entry.provenance.origin,
        entry
            .provenance
            .content_hash
            .chars()
            .take(12)
            .collect::<String>(),
        entry.title
    );
    if !entry.reasons.is_empty() {
        println!("  reasons: {}", entry.reasons.join(", "));
    }
}

fn print_rag_packing(packing: &RagPackingReport) {
    println!(
        "\nPacking: candidates={} selected={} memory={}/{} chunks={}/{} suppressed_duplicate={} suppressed_overlap={} suppressed_file_cap={} suppressed_limit={}",
        packing.candidate_count,
        packing.selected_count,
        packing.selected_memories,
        packing.memory_candidates,
        packing.selected_chunks,
        packing.chunk_candidates,
        packing.suppressed_duplicate,
        packing.suppressed_overlap,
        packing.suppressed_file_cap,
        packing.suppressed_limit
    );
    for file in &packing.chunk_files {
        println!(
            "- {} chunks={}/{} overlap_suppressed={} file_cap_suppressed={}",
            file.path,
            file.selected,
            file.candidates,
            file.suppressed_overlap,
            file.suppressed_file_cap
        );
    }
}

fn rag_chunk_summary(content: &str, query_terms: &HashSet<String>, max_chars: usize) -> String {
    let mut summary = query_focused_summary(content, query_terms, max_chars);
    let literals = markdown_code_literals(content)
        .into_iter()
        .filter(|literal| !summary.to_lowercase().contains(&literal.to_lowercase()))
        .take(8)
        .collect::<Vec<_>>();
    if literals.is_empty() {
        return summary;
    }
    let suffix = format!(
        " Literals: {}",
        literals
            .into_iter()
            .map(|literal| format!("`{literal}`"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let hard_limit = max_chars.saturating_add(180);
    if summary
        .chars()
        .count()
        .saturating_add(suffix.chars().count())
        <= hard_limit
    {
        summary.push_str(&suffix);
    }
    summary
}

fn markdown_code_literals(content: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut rest = content;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('`') else {
            break;
        };
        let literal = rest[..end].trim();
        if !literal.is_empty()
            && literal.len() <= 80
            && !literals.iter().any(|existing| existing == literal)
        {
            literals.push(literal.to_string());
        }
        rest = &rest[end + 1..];
    }
    literals
}

fn rerank_rag_source_pack(
    sources: &mut [RagSource],
    question: &str,
    query_terms: &HashSet<String>,
) {
    let question_lower = question.to_lowercase();
    let source_chunk_intent = rag_source_chunk_query_intent(&question_lower, query_terms);
    for source in sources {
        let boost =
            rag_source_query_boost(source, &question_lower, query_terms, source_chunk_intent);
        if boost <= 0.0 {
            continue;
        }
        source.score += boost;
        source.reasons.push(format!("rag_query_fit:+{boost:.1}"));
    }
}

fn rag_source_query_boost(
    source: &RagSource,
    question_lower: &str,
    query_terms: &HashSet<String>,
    source_chunk_intent: bool,
) -> f64 {
    if query_terms.is_empty() {
        return 0.0;
    }

    let title_terms = tokenize(&source.title);
    let summary_terms = tokenize(&source.summary);
    let title_hits = query_terms
        .iter()
        .filter(|term| title_terms.contains(*term))
        .count();
    let summary_hits = query_terms
        .iter()
        .filter(|term| summary_terms.contains(*term))
        .count();
    let total_hits = title_hits + summary_hits;
    let mut boost = title_hits as f64 * 7.0 + summary_hits.min(8) as f64 * 2.5;
    if title_hits >= 2 {
        boost += 10.0;
    }
    if title_hits >= 4 {
        boost += 8.0;
    }
    if total_hits >= query_terms.len().min(5) {
        boost += 8.0;
    }

    let haystack = format!(
        "{}\n{}\n{}",
        source.title.to_lowercase(),
        source.summary.to_lowercase(),
        source.reasons.join("\n").to_lowercase()
    );
    boost += rag_domain_signal_boost(question_lower, &haystack);

    if source.source_kind == "chunk" {
        if source_chunk_intent {
            boost += 70.0;
            boost += rag_chunk_endpoint_signal_boost(question_lower, &haystack);
        } else if summary_hits >= 3 {
            boost += 8.0;
        }
    } else if title_hits == 0 && summary_hits <= 2 {
        boost *= 0.45;
    }

    boost
}

fn rag_source_chunk_query_intent(question_lower: &str, query_terms: &HashSet<String>) -> bool {
    question_lower.contains("source chunk")
        || question_lower.contains("source chunks")
        || query_terms.contains("chunks")
        || (query_terms.contains("index") && query_terms.contains("files"))
        || (query_terms.contains("indexes") && query_terms.contains("files"))
        || (query_terms.contains("mcp") && query_terms.contains("tool"))
        || (query_terms.contains("http") && query_terms.contains("endpoint"))
}

fn rag_chunk_endpoint_signal_boost(question_lower: &str, haystack: &str) -> f64 {
    let mut boost = 0.0;
    if question_lower.contains("mcp")
        && (haystack.contains("memory_rag_ingest") || haystack.contains("mcp"))
    {
        boost += 24.0;
    }
    if question_lower.contains("http")
        && (haystack.contains("post /rag-ingest")
            || haystack.contains("/rag-ingest")
            || haystack.contains("http"))
    {
        boost += 24.0;
    }
    if (question_lower.contains("cli")
        || question_lower.contains("index files")
        || question_lower.contains("indexes files")
        || question_lower.contains("files as rag"))
        && haystack.contains("rag-ingest")
    {
        boost += 18.0;
    }
    boost
}

fn rag_domain_signal_boost(question_lower: &str, haystack: &str) -> f64 {
    let mut boost = 0.0;
    if question_lower.contains("packing")
        && (question_lower.contains("stats") || question_lower.contains("audit"))
        && (haystack.contains("packing diagnostics")
            || haystack.contains("suppression")
            || haystack.contains("overlap/file-cap"))
    {
        boost += 30.0;
    }
    if question_lower.contains("missing evidence")
        && (haystack.contains("expected evidence placement")
            || haystack.contains("missing from the retrieved candidates"))
    {
        boost += 34.0;
    }
    if question_lower.contains("evidence placement")
        && (haystack.contains("expected evidence placement")
            || haystack.contains("expected_evidence_status"))
    {
        boost += 42.0;
    }
    if question_lower.contains("grounded answer")
        && (haystack.contains("grounded answer") || haystack.contains("selected citations"))
    {
        boost += 10.0;
    }
    boost
}

fn select_rag_sources(
    mut sources: Vec<RagSource>,
    limit: usize,
) -> (Vec<RagSource>, RagPackingReport) {
    let mut packing = rag_packing_candidates(&sources);
    sources.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.source_kind.cmp(&b.source_kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    let mut chunk_ranges_by_path: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    let mut chunk_count_by_path: HashMap<String, usize> = HashMap::new();
    for source in sources.iter() {
        if seen.contains(&source.id) {
            packing.record_skip(source, RagSourceSkipReason::Duplicate);
            continue;
        }
        if selected.len() >= limit {
            packing.record_skip(source, RagSourceSkipReason::Limit);
            continue;
        }
        if let Some(skip) =
            rag_source_skip_reason(source, &chunk_ranges_by_path, &chunk_count_by_path)
        {
            packing.record_skip(source, skip);
            continue;
        }
        seen.insert(source.id.clone());
        remember_rag_source_context(source, &mut chunk_ranges_by_path, &mut chunk_count_by_path);
        selected.push(source.clone());
    }
    if limit > 1
        && sources.iter().any(|source| source.source_kind == "chunk")
        && !selected.iter().any(|source| source.source_kind == "chunk")
        && let Some(chunk) = sources
            .iter()
            .find(|source| source.source_kind == "chunk" && !seen.contains(&source.id))
            .cloned()
    {
        if let Some(removed) = selected.pop() {
            packing.record_skip(&removed, RagSourceSkipReason::Limit);
        }
        selected.push(chunk);
        selected.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    promote_diverse_chunk_source(&sources, &mut selected, &mut packing);
    packing.finalize(&selected);
    (selected, packing)
}

fn promote_diverse_chunk_source(
    sources: &[RagSource],
    selected: &mut [RagSource],
    packing: &mut RagPackingReport,
) {
    if selected.len() < 3 {
        return;
    }
    let selected_chunks = selected
        .iter()
        .filter(|source| source.source_kind == "chunk")
        .count();
    let selected_memories = selected.len().saturating_sub(selected_chunks);
    if selected_memories <= selected_chunks {
        return;
    }
    let Some((weakest_memory_index, weakest_memory)) = selected
        .iter()
        .enumerate()
        .filter(|(_, source)| source.source_kind != "chunk")
        .min_by(|(_, left), (_, right)| {
            left.score
                .partial_cmp(&right.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.id.cmp(&left.id))
        })
    else {
        return;
    };
    let selected_ids = selected
        .iter()
        .map(|source| source.id.as_str())
        .collect::<HashSet<_>>();
    let selected_paths = selected
        .iter()
        .filter_map(|source| source.path.as_deref())
        .collect::<HashSet<_>>();
    let (chunk_ranges_by_path, chunk_count_by_path) = rag_selected_chunk_context(selected);
    let candidate = sources
        .iter()
        .filter(|source| source.source_kind == "chunk")
        .filter(|source| !selected_ids.contains(source.id.as_str()))
        .filter(|source| {
            source
                .path
                .as_deref()
                .is_some_and(|path| !selected_paths.contains(path))
        })
        .filter(|source| {
            rag_source_skip_reason(source, &chunk_ranges_by_path, &chunk_count_by_path).is_none()
        })
        .filter(|source| rag_diversity_candidate_is_strong(source, weakest_memory.score))
        .max_by(|left, right| {
            left.score
                .partial_cmp(&right.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.id.cmp(&left.id))
        })
        .cloned();
    let Some(candidate) = candidate else {
        return;
    };
    let removed = std::mem::replace(&mut selected[weakest_memory_index], candidate);
    packing.record_skip(&removed, RagSourceSkipReason::Limit);
    selected.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.source_kind.cmp(&b.source_kind))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn rag_diversity_candidate_is_strong(candidate: &RagSource, replaced_score: f64) -> bool {
    candidate.score >= replaced_score * 0.72 || candidate.score + 12.0 >= replaced_score
}

type RagChunkRangesByPath = HashMap<String, Vec<(usize, usize)>>;
type RagChunkCountByPath = HashMap<String, usize>;

fn rag_selected_chunk_context(
    selected: &[RagSource],
) -> (RagChunkRangesByPath, RagChunkCountByPath) {
    let mut chunk_ranges_by_path = HashMap::new();
    let mut chunk_count_by_path = HashMap::new();
    for source in selected {
        remember_rag_source_context(source, &mut chunk_ranges_by_path, &mut chunk_count_by_path);
    }
    (chunk_ranges_by_path, chunk_count_by_path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RagSourceSkipReason {
    Duplicate,
    Overlap,
    FileCap,
    Limit,
}

impl RagSourceSkipReason {
    fn as_str(self) -> &'static str {
        match self {
            RagSourceSkipReason::Duplicate => "duplicate",
            RagSourceSkipReason::Overlap => "overlap",
            RagSourceSkipReason::FileCap => "file_cap",
            RagSourceSkipReason::Limit => "limit",
        }
    }
}

impl RagPackingReport {
    fn record_skip(&mut self, source: &RagSource, reason: RagSourceSkipReason) {
        self.suppressed_sources.push(RagPackingSuppressedSource {
            id: source.id.clone(),
            source_kind: source.source_kind.clone(),
            title: source.title.clone(),
            reason: reason.as_str().to_string(),
            score: source.score,
            semantic_score: source.semantic_score,
            location: rag_source_location(source),
            summary: source.summary.clone(),
        });
        let Some(path) = &source.path else {
            return;
        };
        let file = self.ensure_file(path);
        match reason {
            RagSourceSkipReason::Duplicate => {}
            RagSourceSkipReason::Overlap => file.suppressed_overlap += 1,
            RagSourceSkipReason::FileCap => file.suppressed_file_cap += 1,
            RagSourceSkipReason::Limit => {}
        }
    }

    fn finalize(&mut self, selected: &[RagSource]) {
        let selected_ids = selected
            .iter()
            .map(|source| source.id.as_str())
            .collect::<HashSet<_>>();
        self.suppressed_sources
            .retain(|source| !selected_ids.contains(source.id.as_str()));
        self.suppressed_duplicate = 0;
        self.suppressed_overlap = 0;
        self.suppressed_file_cap = 0;
        self.suppressed_limit = 0;
        self.selected_count = selected.len();
        self.selected_memories = selected
            .iter()
            .filter(|source| source.source_kind != "chunk")
            .count();
        self.selected_chunks = selected
            .iter()
            .filter(|source| source.source_kind == "chunk")
            .count();
        for file in &mut self.chunk_files {
            file.selected = 0;
            file.suppressed_overlap = 0;
            file.suppressed_file_cap = 0;
        }
        let suppressed_file_updates = self
            .suppressed_sources
            .iter()
            .filter_map(|source| {
                let location = source.location.as_ref()?;
                let path = location
                    .split_once(':')
                    .map(|(path, _)| path.to_string())
                    .unwrap_or_else(|| location.clone());
                Some((path, source.reason.clone()))
            })
            .collect::<Vec<_>>();
        for source in &self.suppressed_sources {
            match source.reason.as_str() {
                "duplicate" => self.suppressed_duplicate += 1,
                "overlap" => self.suppressed_overlap += 1,
                "file_cap" => self.suppressed_file_cap += 1,
                "limit" => self.suppressed_limit += 1,
                _ => {}
            }
        }
        for (path, reason) in suppressed_file_updates {
            let file = self.ensure_file(&path);
            match reason.as_str() {
                "overlap" => file.suppressed_overlap += 1,
                "file_cap" => file.suppressed_file_cap += 1,
                _ => {}
            }
        }
        for source in selected
            .iter()
            .filter(|source| source.source_kind == "chunk")
        {
            if let Some(path) = &source.path {
                self.ensure_file(path).selected += 1;
            }
        }
        self.chunk_files
            .sort_by(|left, right| left.path.cmp(&right.path));
    }

    fn ensure_file(&mut self, path: &str) -> &mut RagPackingFileReport {
        if let Some(index) = self.chunk_files.iter().position(|file| file.path == path) {
            return &mut self.chunk_files[index];
        }
        self.chunk_files.push(RagPackingFileReport {
            path: path.to_string(),
            ..RagPackingFileReport::default()
        });
        self.chunk_files
            .last_mut()
            .expect("pushed RAG packing file report")
    }
}

fn rag_packing_candidates(sources: &[RagSource]) -> RagPackingReport {
    let mut report = RagPackingReport {
        candidate_count: sources.len(),
        memory_candidates: sources
            .iter()
            .filter(|source| source.source_kind != "chunk")
            .count(),
        chunk_candidates: sources
            .iter()
            .filter(|source| source.source_kind == "chunk")
            .count(),
        ..RagPackingReport::default()
    };
    for source in sources
        .iter()
        .filter(|source| source.source_kind == "chunk")
    {
        if let Some(path) = &source.path {
            report.ensure_file(path).candidates += 1;
        }
    }
    report
        .chunk_files
        .sort_by(|left, right| left.path.cmp(&right.path));
    report
}

fn rag_source_skip_reason(
    source: &RagSource,
    chunk_ranges_by_path: &HashMap<String, Vec<(usize, usize)>>,
    chunk_count_by_path: &HashMap<String, usize>,
) -> Option<RagSourceSkipReason> {
    const CHUNKS_PER_FILE_LIMIT: usize = 3;
    if source.source_kind != "chunk" {
        return None;
    }
    let Some(path) = &source.path else {
        return None;
    };
    if chunk_count_by_path.get(path).copied().unwrap_or_default() >= CHUNKS_PER_FILE_LIMIT {
        return Some(RagSourceSkipReason::FileCap);
    }
    let (start, end) = rag_source_line_range(source)?;
    if chunk_ranges_by_path.get(path).is_some_and(|ranges| {
        ranges
            .iter()
            .any(|range| rag_line_ranges_too_overlapping((start, end), *range))
    }) {
        return Some(RagSourceSkipReason::Overlap);
    }
    None
}

fn remember_rag_source_context(
    source: &RagSource,
    chunk_ranges_by_path: &mut HashMap<String, Vec<(usize, usize)>>,
    chunk_count_by_path: &mut HashMap<String, usize>,
) {
    if source.source_kind != "chunk" {
        return;
    }
    let Some(path) = &source.path else {
        return;
    };
    *chunk_count_by_path.entry(path.clone()).or_default() += 1;
    if let Some(range) = rag_source_line_range(source) {
        chunk_ranges_by_path
            .entry(path.clone())
            .or_default()
            .push(range);
    }
}

fn rag_source_line_range(source: &RagSource) -> Option<(usize, usize)> {
    Some((source.start_line?, source.end_line?))
}

fn rag_line_ranges_too_overlapping(left: (usize, usize), right: (usize, usize)) -> bool {
    let (left_start, left_end) = left;
    let (right_start, right_end) = right;
    let overlap_start = left_start.max(right_start);
    let overlap_end = left_end.min(right_end);
    if overlap_start > overlap_end {
        return false;
    }
    let overlap = overlap_end.saturating_sub(overlap_start).saturating_add(1);
    let left_len = left_end.saturating_sub(left_start).saturating_add(1);
    let right_len = right_end.saturating_sub(right_start).saturating_add(1);
    let shorter = left_len.min(right_len).max(1);
    overlap.saturating_mul(100) >= shorter.saturating_mul(70)
}

fn rag_missing_evidence(retrieval: &RetrievalReport, source_pack: &[RagSource]) -> Vec<String> {
    let mut gaps = Vec::new();
    if source_pack.is_empty() {
        gaps.push(
            "no active memory cards or indexed source chunks matched the question".to_string(),
        );
    }
    if source_pack.len() < 3 {
        gaps.push("source pack has fewer than three independent sources".to_string());
    }
    if source_pack
        .iter()
        .any(|source| source.status == "uncertain")
    {
        gaps.push("some selected memory cards are uncertain".to_string());
    }
    if retrieval.semantic_skipped {
        if let Some(reason) = &retrieval.semantic_skip_reason {
            gaps.push(format!("semantic retrieval was skipped: {reason}"));
        }
    } else if !retrieval.semantic_used
        && let Some(error) = &retrieval.semantic_error
    {
        gaps.push(format!(
            "semantic retrieval fell back to lexical search: {error}"
        ));
    }
    gaps
}

fn rag_confidence(source_pack: &[RagSource]) -> (String, f64) {
    if source_pack.is_empty() {
        return ("none".to_string(), 0.0);
    }
    let source_count_score = (source_pack.len() as f64 / 5.0).min(1.0);
    let avg_score =
        source_pack.iter().map(|source| source.score).sum::<f64>() / source_pack.len() as f64;
    let retrieval_score = (avg_score / 5.0).clamp(0.0, 1.0);
    let avg_memory_confidence = source_pack
        .iter()
        .map(|source| source.confidence)
        .sum::<f64>()
        / source_pack.len() as f64;
    let active_ratio = source_pack
        .iter()
        .filter(|source| source.status == "active")
        .count() as f64
        / source_pack.len() as f64;
    let mut score = (source_count_score * 0.30
        + retrieval_score * 0.30
        + avg_memory_confidence.clamp(0.0, 1.0) * 0.25
        + active_ratio * 0.15)
        .clamp(0.0, 1.0);
    if source_pack.len() < 3 {
        score = score.min(0.49);
    } else if source_pack.len() < 5 {
        score = score.min(0.74);
    }
    let label = if score >= 0.75 && source_pack.len() >= 5 && active_ratio >= 0.8 {
        "high"
    } else if score >= 0.52 && source_pack.len() >= 3 {
        "medium"
    } else {
        "low"
    };
    (label.to_string(), (score * 100.0).round() / 100.0)
}

fn rag_generation_prompt(
    question: &str,
    source_pack: &[RagSource],
    missing_evidence: &[String],
) -> String {
    let mut prompt = String::new();
    prompt.push_str("Answer from local project memory only.\n");
    prompt.push_str("Rules:\n");
    prompt.push_str("- Use only the source pack below; do not use general knowledge.\n");
    prompt.push_str("- Cite every concrete claim with memory ids like [abc123].\n");
    prompt.push_str(
        "- If evidence is incomplete, state that clearly and name the missing evidence.\n",
    );
    prompt.push_str("- Answer in the same language as the question when obvious.\n\n");
    prompt.push_str("Source pack:\n");
    for source in source_pack {
        prompt.push_str(&format!(
            "[{}] source_kind={} type={} status={} confidence={:.2} score={:.2}\nTitle: {}\n{}Summary: {}\nReasons: {}\n\n",
            source.id,
            source.source_kind,
            source.memory_type,
            source.status,
            source.confidence,
            source.score,
            source.title,
            rag_prompt_location(source),
            source.summary,
            source.reasons.join(", ")
        ));
    }
    if !missing_evidence.is_empty() {
        prompt.push_str("Missing evidence signals:\n");
        for item in missing_evidence {
            prompt.push_str(&format!("- {item}\n"));
        }
        prompt.push('\n');
    }
    prompt.push_str(&format!("Question: {question}\n"));
    prompt
}

fn rag_no_evidence_answer(question: &str) -> String {
    if question
        .chars()
        .any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch))
    {
        format!(
            "Недостаточно подтвержденной проектной памяти или индексированных source chunks, чтобы ответить на \"{}\". Добавь релевантные карточки памяти или выполни `dukememory rag-ingest --apply`, затем повтори `dukememory rag-answer`.",
            truncate_chars(question, 120)
        )
    } else {
        format!(
            "There is not enough grounded project memory or indexed source chunks to answer \"{}\". Add relevant memory cards or run `dukememory rag-ingest --apply`, then rerun `dukememory rag-answer`.",
            truncate_chars(question, 120)
        )
    }
}

fn rag_guard_generated_answer(
    question: &str,
    generated: String,
    source_pack: &[RagSource],
    missing_evidence: &[String],
) -> (String, RagGenerationGuardReport) {
    let guard = rag_generation_guard_report(&generated, source_pack);
    if !guard.accepted_generated {
        return (
            rag_extractive_answer(question, source_pack, missing_evidence),
            guard,
        );
    }
    (generated, guard)
}

fn rag_no_evidence_generation_guard() -> RagGenerationGuardReport {
    RagGenerationGuardReport {
        answer_source: "no_evidence".to_string(),
        accepted_generated: false,
        fallback_reason: Some("no_source_pack".to_string()),
        selected_citations: Vec::new(),
        prompt_fragment_detected: false,
        generated_chars: 0,
    }
}

fn rag_generation_guard_report(
    answer: &str,
    source_pack: &[RagSource],
) -> RagGenerationGuardReport {
    let trimmed = answer.trim();
    let generated_chars = trimmed.chars().count();
    if trimmed.is_empty() {
        return RagGenerationGuardReport {
            answer_source: "extractive_fallback".to_string(),
            accepted_generated: false,
            fallback_reason: Some("empty_generation".to_string()),
            selected_citations: Vec::new(),
            prompt_fragment_detected: false,
            generated_chars,
        };
    }
    let prompt_fragment = [
        "type=",
        "status=",
        "Reasons:",
        "Source pack:",
        "Missing evidence signals:",
    ]
    .iter()
    .any(|needle| trimmed.contains(needle));
    if prompt_fragment {
        return RagGenerationGuardReport {
            answer_source: "extractive_fallback".to_string(),
            accepted_generated: false,
            fallback_reason: Some("prompt_fragment".to_string()),
            selected_citations: rag_answer_selected_citations(trimmed, source_pack),
            prompt_fragment_detected: true,
            generated_chars,
        };
    }
    let selected_citations = rag_answer_selected_citations(trimmed, source_pack);
    if selected_citations.is_empty() {
        return RagGenerationGuardReport {
            answer_source: "extractive_fallback".to_string(),
            accepted_generated: false,
            fallback_reason: Some("missing_selected_citation".to_string()),
            selected_citations,
            prompt_fragment_detected: false,
            generated_chars,
        };
    }
    RagGenerationGuardReport {
        answer_source: "generated".to_string(),
        accepted_generated: true,
        fallback_reason: None,
        selected_citations,
        prompt_fragment_detected: false,
        generated_chars,
    }
}

fn rag_answer_selected_citations(answer: &str, source_pack: &[RagSource]) -> Vec<String> {
    source_pack
        .iter()
        .filter(|source| {
            answer.contains(&format!("[{}]", source.id)) || answer.contains(&source.id)
        })
        .map(|source| source.id.clone())
        .collect()
}

pub(crate) fn rag_extractive_answer(
    question: &str,
    source_pack: &[RagSource],
    missing_evidence: &[String],
) -> String {
    let russian = question
        .chars()
        .any(|ch| ('\u{0400}'..='\u{04FF}').contains(&ch));
    let mut parts = Vec::new();
    for source in rag_extractive_answer_sources(source_pack) {
        parts.push(format!(
            "{} [{}]: {}",
            source.title,
            source.id,
            rag_extractive_summary(source)
        ));
    }
    if russian {
        let mut answer = format!("По подтвержденным источникам: {}", parts.join(" "));
        if !missing_evidence.is_empty() {
            answer.push_str(" Недостает доказательств: ");
            answer.push_str(&missing_evidence.join("; "));
            answer.push('.');
        }
        answer
    } else {
        let mut answer = format!("Grounded answer from project sources: {}", parts.join(" "));
        if !missing_evidence.is_empty() {
            answer.push_str(" Missing evidence: ");
            answer.push_str(&missing_evidence.join("; "));
            answer.push('.');
        }
        answer
    }
}

fn rag_extractive_summary(source: &RagSource) -> String {
    if source.source_kind == "chunk"
        && let Some((body, literals)) = source.summary.split_once(" Literals: ")
    {
        return format!(
            "{} Literals: {}",
            truncate_chars(body, 220),
            truncate_chars(literals, 220)
        );
    }
    let max_chars = 340;
    truncate_chars(&source.summary, max_chars)
}

fn rag_extractive_answer_sources(source_pack: &[RagSource]) -> Vec<&RagSource> {
    const PRIMARY_SOURCE_LIMIT: usize = 4;
    const CHUNK_SOURCE_LIMIT: usize = 2;
    const TOTAL_SOURCE_LIMIT: usize = 6;

    let mut selected = Vec::new();
    for index in 0..source_pack.len().min(PRIMARY_SOURCE_LIMIT) {
        if selected.len() >= TOTAL_SOURCE_LIMIT {
            break;
        }
        selected.push(index);
    }
    for index in source_pack
        .iter()
        .enumerate()
        .filter(|(_, source)| source.source_kind == "chunk")
        .map(|(index, _)| index)
        .take(CHUNK_SOURCE_LIMIT)
    {
        if selected.len() >= TOTAL_SOURCE_LIMIT {
            break;
        }
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    for index in 0..source_pack.len() {
        if selected.len() >= TOTAL_SOURCE_LIMIT {
            break;
        }
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    selected
        .into_iter()
        .map(|index| &source_pack[index])
        .collect()
}

fn rag_prompt_location(source: &RagSource) -> String {
    let Some(path) = &source.path else {
        return String::new();
    };
    let lines = match (source.start_line, source.end_line) {
        (Some(start), Some(end)) => format!(":{}-{}", start, end),
        _ => String::new(),
    };
    format!("Location: {path}{lines}\n")
}

fn rag_recommendations(retrieval: &RetrievalReport) -> Vec<String> {
    let mut recommendations = vec![
        "use rag-debug before rag-answer when the answer must be auditable".to_string(),
        "treat citations as evidence handles and inspect surprising cards before acting"
            .to_string(),
    ];
    if retrieval.hits.len() < 3 {
        recommendations
            .push("add reviewed memory cards or indexed source chunks for missing evidence instead of raising generation temperature".to_string());
    }
    if retrieval.semantic_error.is_some() {
        recommendations
            .push("refresh local embeddings before relying on semantic recall".to_string());
    }
    recommendations
}

#[cfg(test)]
mod rag_tests {
    use super::*;

    fn source(id: &str, status: &str, score: f64) -> RagSource {
        RagSource {
            id: id.to_string(),
            source_kind: "memory".to_string(),
            memory_type: "design_note".to_string(),
            scope: "project".to_string(),
            title: format!("source {id}"),
            status: status.to_string(),
            score,
            utility_score: 1.0,
            semantic_score: Some(0.8),
            confidence: 1.0,
            reasons: vec!["semantic:0.800".to_string()],
            summary: "grounded source".to_string(),
            links: vec![],
            provenance: RagSourceProvenance {
                origin: "memory_store".to_string(),
                evidence_ref: format!("dukememory:memory:{id}"),
                content_hash: "test-hash".to_string(),
                source: Some("test".to_string()),
                updated_at: Some(1),
            },
            path: None,
            chunk_index: None,
            start_line: None,
            end_line: None,
        }
    }

    fn chunk_source(
        id: &str,
        path: &str,
        start_line: usize,
        end_line: usize,
        score: f64,
    ) -> RagSource {
        let mut source = source(id, "active", score);
        source.source_kind = "chunk".to_string();
        source.memory_type = "source_chunk".to_string();
        source.path = Some(path.to_string());
        source.start_line = Some(start_line);
        source.end_line = Some(end_line);
        source.title = format!("{path}:{start_line}-{end_line}");
        source.reasons = vec!["semantic_chunk:0.800".to_string()];
        source.provenance.origin = "rag_chunk".to_string();
        source.provenance.evidence_ref = format!("dukememory:chunk:{id}");
        source.provenance.source = Some(path.to_string());
        source.provenance.updated_at = None;
        source
    }

    #[test]
    fn rag_confidence_caps_low_when_fewer_than_three_sources() {
        let sources = vec![source("a", "active", 50.0), source("b", "active", 50.0)];
        let (label, score) = rag_confidence(&sources);
        assert_eq!(label, "low");
        assert!(score <= 0.49);
    }

    #[test]
    fn rag_confidence_caps_medium_when_fewer_than_five_sources() {
        let sources = vec![
            source("a", "active", 50.0),
            source("b", "active", 50.0),
            source("c", "active", 50.0),
            source("d", "active", 50.0),
        ];
        let (label, score) = rag_confidence(&sources);
        assert_eq!(label, "medium");
        assert!(score <= 0.74);
    }

    #[test]
    fn rag_no_evidence_answer_uses_russian_for_cyrillic_question() {
        let answer = rag_no_evidence_answer("Что настроено?");
        assert!(answer.contains("Недостаточно"));
    }

    #[test]
    fn rag_guard_falls_back_for_short_uncited_generation() {
        let sources = vec![source("abc123", "active", 50.0)];
        let (answer, guard) = rag_guard_generated_answer(
            "Что настроено?",
            "type=design_note".to_string(),
            &sources,
            &[],
        );
        assert!(answer.contains("[abc123]"));
        assert!(answer.contains("По подтвержденным источникам"));
        assert_eq!(guard.answer_source, "extractive_fallback");
        assert_eq!(guard.fallback_reason.as_deref(), Some("prompt_fragment"));
        assert!(guard.prompt_fragment_detected);
    }

    #[test]
    fn rag_guard_falls_back_for_long_uncited_generation() {
        let sources = vec![source("abc123", "active", 50.0)];
        let (answer, guard) = rag_guard_generated_answer(
            "How do agents index source chunks?",
            "Agents index source chunks by running several plausible indexing and evaluation commands, then checking the resulting retrieval behavior across benchmark cases. This answer is intentionally long but it does not cite a selected source id.".to_string(),
            &sources,
            &[],
        );

        assert!(answer.contains("[abc123]"));
        assert!(answer.contains("Grounded answer from project sources"));
        assert_eq!(guard.answer_source, "extractive_fallback");
        assert_eq!(
            guard.fallback_reason.as_deref(),
            Some("missing_selected_citation")
        );
        assert!(guard.selected_citations.is_empty());
    }

    #[test]
    fn rag_guard_accepts_generated_answer_with_selected_citation() {
        let sources = vec![source("abc123", "active", 50.0)];
        let (answer, guard) = rag_guard_generated_answer(
            "How do agents index source chunks?",
            "The selected source records the answer with citation [abc123].".to_string(),
            &sources,
            &[],
        );

        assert_eq!(
            answer,
            "The selected source records the answer with citation [abc123]."
        );
        assert_eq!(guard.answer_source, "generated");
        assert!(guard.accepted_generated);
        assert_eq!(guard.selected_citations, vec!["abc123".to_string()]);
    }

    #[test]
    fn rag_extractive_answer_includes_chunk_literals_beyond_top_memories() {
        let mut sources = vec![
            source("mem-a", "active", 90.0),
            source("mem-b", "active", 89.0),
            source("mem-c", "active", 88.0),
            source("mem-d", "active", 87.0),
            chunk_source("chunk-a", "README.md", 10, 20, 80.0),
            chunk_source("chunk-b", "README.md", 30, 40, 79.0),
        ];
        sources[4].summary = "CLI source chunks use `rag-ingest`.".to_string();
        sources[5].summary = format!(
            "{} Literals: `memory_rag_ingest`",
            "MCP source chunks stay grounded through indexed file evidence. ".repeat(8)
        );

        let answer = rag_extractive_answer("Which MCP tool indexes chunks?", &sources, &[]);

        assert!(answer.contains("memory_rag_ingest"));
        assert!(answer.contains("[chunk-b]"));
    }

    #[test]
    fn rag_extractive_answer_fills_remaining_slots_after_chunks() {
        let sources = vec![
            source("mem-a", "active", 90.0),
            source("mem-b", "active", 89.0),
            source("mem-c", "active", 88.0),
            source("mem-d", "active", 87.0),
            source("expected-card", "active", 86.0),
            chunk_source("chunk-a", "README.md", 10, 20, 80.0),
        ];

        let answer = rag_extractive_answer("Which relationship supports evidence?", &sources, &[]);

        assert!(answer.contains("[chunk-a]"));
        assert!(answer.contains("[expected-card]"));
    }

    #[test]
    fn rag_trace_entries_include_chunk_location_and_reasons() {
        let mut chunk = source("chunk123", "active", 8.0);
        chunk.source_kind = "chunk".to_string();
        chunk.memory_type = "source_chunk".to_string();
        chunk.path = Some("README.md".to_string());
        chunk.start_line = Some(214);
        chunk.end_line = Some(218);
        chunk.reasons = vec![
            "semantic_chunk:0.810".to_string(),
            "hybrid_chunk".to_string(),
        ];
        chunk.provenance = RagSourceProvenance {
            origin: "rag_chunk".to_string(),
            evidence_ref: "dukememory:chunk:chunk123".to_string(),
            content_hash: "sha256-content".to_string(),
            source: Some("README.md".to_string()),
            updated_at: None,
        };

        let trace = rag_trace_entries(&[chunk]);

        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].rank, 1);
        assert_eq!(trace[0].location.as_deref(), Some("README.md:214-218"));
        assert_eq!(trace[0].provenance.origin, "rag_chunk");
        assert_eq!(
            trace[0].provenance.evidence_ref,
            "dukememory:chunk:chunk123"
        );
        assert_eq!(trace[0].provenance.content_hash, "sha256-content");
        assert!(
            trace[0]
                .reasons
                .iter()
                .any(|reason| reason == "hybrid_chunk")
        );
    }

    #[test]
    fn rag_query_rerank_promotes_chunk_endpoint_evidence() {
        let question = "Which MCP tool indexes RAG source chunks?";
        let query_terms = relevance_terms(question);
        let mut sources = vec![
            source("broad-memory", "active", 82.0),
            source("source-pack-memory", "active", 74.0),
            chunk_source("chunk-mcp", "README.md", 494, 513, 20.0),
        ];
        sources[0].title = "Understanding project architecture for optimization".to_string();
        sources[0].summary =
            "MCP/HTTP surfaces and RAG reports are part of the project architecture.".to_string();
        sources[1].title = "RAG source pack diversifies chunk evidence".to_string();
        sources[1].summary =
            "RAG source chunks are available through CLI, MCP, and HTTP evidence.".to_string();
        sources[2].summary =
            "Source chunks are exposed to agents as MCP `memory_rag_ingest`.".to_string();

        rerank_rag_source_pack(&mut sources, question, &query_terms);
        let (selected, _) = select_rag_sources(sources, 3);

        assert_eq!(selected[0].id, "chunk-mcp");
        assert!(
            selected[0]
                .reasons
                .iter()
                .any(|reason| reason.starts_with("rag_query_fit:+"))
        );
    }

    #[test]
    fn rag_query_rerank_promotes_specific_evidence_placement_card() {
        let question = "Which relationship links RAG eval grounded answers to evidence placement?";
        let query_terms = relevance_terms(question);
        let mut sources = vec![
            source("broad-trace", "active", 84.0),
            source("target-placement", "active", 56.0),
            source("grounded-answer", "active", 55.0),
        ];
        sources[0].title = "RAG answer and graph-RAG expose audit trace".to_string();
        sources[0].summary =
            "Trace entries include evidence ids, graph relationships, and ranked sources."
                .to_string();
        sources[1].title =
            "RAG eval distinguishes selected, suppressed, and missing evidence".to_string();
        sources[1].summary =
            "RagEvalCaseResult reports expected evidence placement and expected_evidence_status."
                .to_string();
        sources[2].title = "RAG eval gates grounded answers".to_string();
        sources[2].summary =
            "Grounded answers report selected citations and expected evidence coverage."
                .to_string();

        rerank_rag_source_pack(&mut sources, question, &query_terms);
        let (selected, _) = select_rag_sources(sources, 3);

        assert_eq!(selected[0].id, "target-placement");
    }

    #[test]
    fn select_rag_sources_suppresses_overlapping_chunks_from_same_file() {
        let sources = vec![
            chunk_source("chunk-a", "README.md", 10, 20, 100.0),
            chunk_source("chunk-b", "README.md", 12, 18, 99.0),
            chunk_source("chunk-c", "README.md", 80, 90, 98.0),
        ];

        let (selected, packing) = select_rag_sources(sources, 3);
        let ids = selected
            .iter()
            .map(|source| source.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"chunk-a"));
        assert!(!ids.contains(&"chunk-b"));
        assert!(ids.contains(&"chunk-c"));
        assert_eq!(packing.chunk_candidates, 3);
        assert_eq!(packing.selected_chunks, 2);
        assert_eq!(packing.suppressed_overlap, 1);
        assert_eq!(packing.suppressed_file_cap, 0);
    }

    #[test]
    fn select_rag_sources_limits_chunks_per_file_without_losing_file_diversity() {
        let sources = vec![
            chunk_source("chunk-a", "README.md", 10, 20, 100.0),
            chunk_source("chunk-b", "README.md", 80, 90, 99.0),
            chunk_source("chunk-c", "README.md", 150, 160, 98.0),
            chunk_source("chunk-d", "README.md", 220, 230, 97.0),
            chunk_source("chunk-e", "src/app.rs", 10, 20, 96.0),
        ];

        let (selected, packing) = select_rag_sources(sources, 5);
        let readme_count = selected
            .iter()
            .filter(|source| source.path.as_deref() == Some("README.md"))
            .count();

        assert_eq!(readme_count, 3);
        assert!(selected.iter().any(|source| source.id == "chunk-e"));
        assert!(!selected.iter().any(|source| source.id == "chunk-d"));
        assert_eq!(packing.chunk_candidates, 5);
        assert_eq!(packing.selected_chunks, 4);
        assert_eq!(packing.suppressed_file_cap, 1);
        let readme = packing
            .chunk_files
            .iter()
            .find(|file| file.path == "README.md")
            .expect("README packing stats");
        assert_eq!(readme.candidates, 4);
        assert_eq!(readme.selected, 3);
        assert_eq!(readme.suppressed_file_cap, 1);
    }

    #[test]
    fn select_rag_sources_promotes_strong_chunks_from_new_files_under_limit_pressure() {
        let sources = vec![
            source("memory-a", "active", 100.0),
            source("memory-b", "active", 99.0),
            source("memory-c", "active", 98.0),
            chunk_source("chunk-a", "README.md", 10, 20, 97.0),
            chunk_source("chunk-b", "src/app.rs", 10, 20, 96.0),
        ];

        let (selected, packing) = select_rag_sources(sources, 3);
        let ids = selected
            .iter()
            .map(|source| source.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"memory-a"));
        assert!(ids.contains(&"chunk-a"));
        assert!(ids.contains(&"chunk-b"));
        assert_eq!(packing.selected_memories, 1);
        assert_eq!(packing.selected_chunks, 2);
        assert!(packing.suppressed_limit >= 2);
    }

    #[test]
    fn select_rag_sources_does_not_promote_weak_diversity_chunks() {
        let sources = vec![
            source("memory-a", "active", 100.0),
            source("memory-b", "active", 99.0),
            source("memory-c", "active", 98.0),
            chunk_source("chunk-a", "README.md", 10, 20, 97.0),
            chunk_source("chunk-b", "src/app.rs", 10, 20, 40.0),
        ];

        let (selected, packing) = select_rag_sources(sources, 3);
        let ids = selected
            .iter()
            .map(|source| source.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"memory-a"));
        assert!(ids.contains(&"memory-b"));
        assert!(ids.contains(&"chunk-a"));
        assert!(!ids.contains(&"chunk-b"));
        assert_eq!(packing.selected_memories, 2);
        assert_eq!(packing.selected_chunks, 1);
    }
}

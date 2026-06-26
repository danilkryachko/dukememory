use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub db_path: String,
    pub default_context_limit: usize,
    pub default_context_max_chars: usize,
    pub default_statuses: Vec<String>,
    pub embeddings: EmbeddingConfig,
    pub codegraph: CodeGraphConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraphConfig {
    pub enabled: bool,
    pub command: String,
}

impl AgentConfig {
    pub fn production_defaults(
        db_path: &Path,
        provider: &str,
        endpoint: &str,
        model: &str,
    ) -> Self {
        Self {
            db_path: db_path.display().to_string(),
            default_context_limit: 12,
            default_context_max_chars: 4000,
            default_statuses: vec!["active".to_string(), "uncertain".to_string()],
            embeddings: EmbeddingConfig {
                provider: provider.to_string(),
                endpoint: endpoint.to_string(),
                model: model.to_string(),
            },
            codegraph: CodeGraphConfig {
                enabled: true,
                command: "codegraph".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub config_path: PathBuf,
    pub config: AgentConfig,
}

pub fn load_runtime_config(
    explicit_path: Option<&Path>,
    db_path: &Path,
    default_config_path: &str,
    provider: &str,
    endpoint: &str,
    model: &str,
) -> Result<RuntimeConfig> {
    let path = explicit_path
        .map(Path::to_path_buf)
        .or_else(|| std::env::var("DUKEMEMORY_CONFIG").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(default_config_path));
    let mut config = if path.exists() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str::<AgentConfig>(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?
    } else {
        AgentConfig::production_defaults(db_path, provider, endpoint, model)
    };
    if let Ok(value) = std::env::var("DUKEMEMORY_EMBED_PROVIDER") {
        config.embeddings.provider = value;
    }
    if let Ok(value) = std::env::var("DUKEMEMORY_EMBED_ENDPOINT") {
        config.embeddings.endpoint = value;
    }
    if let Ok(value) = std::env::var("DUKEMEMORY_EMBED_MODEL") {
        config.embeddings.model = value;
    }
    Ok(RuntimeConfig {
        config_path: path,
        config,
    })
}

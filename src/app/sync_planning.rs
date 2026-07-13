use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct SyncLatencyReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) local_first: bool,
    pub(crate) samples: usize,
    pub(crate) local_db_bytes: u64,
    pub(crate) local_read_ms: u128,
    pub(crate) target: Option<String>,
    pub(crate) target_write_ms: Option<u128>,
    pub(crate) target_read_ms: Option<u128>,
    pub(crate) estimated_roundtrip_ms: u32,
    pub(crate) recommended_mode: String,
    pub(crate) issues: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncProfileReport {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) profile: String,
    pub(crate) applied: bool,
    pub(crate) local_first: bool,
    pub(crate) target: Option<String>,
    pub(crate) latency: SyncLatencyReport,
    pub(crate) commands: Vec<String>,
    pub(crate) flow_steps: Vec<SyncProfileFlowStep>,
    pub(crate) actions: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SyncProfileFlowStep {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoteSyncV2Report {
    pub(crate) version: u32,
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) root: String,
    pub(crate) target: Option<String>,
    pub(crate) applied: bool,
    pub(crate) experimental: bool,
    pub(crate) plan_only: bool,
    pub(crate) executed: bool,
    pub(crate) local_first: bool,
    pub(crate) encrypted_bundle: bool,
    pub(crate) encryption_mode: String,
    pub(crate) latency: SyncLatencyReport,
    pub(crate) conflict_policy: String,
    pub(crate) commands: Vec<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) blockers: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

pub(crate) fn remote_sync_v2_commands(target: Option<&str>) -> Vec<String> {
    let target = target.unwrap_or("TARGET");
    vec![
        "dukememory sync export memory-sync.json --json".to_string(),
        "openssl enc -aes-256-cbc -pbkdf2 -salt -in memory-sync.json -out memory-sync.json.enc -pass env:DUKEMEMORY_SYNC_PASSPHRASE".to_string(),
        format!("install -m 600 memory-sync.json.enc {target}/dukememory-sync-bundle.json.enc"),
        format!("openssl enc -d -aes-256-cbc -pbkdf2 -in {target}/dukememory-sync-bundle.json.enc -out memory-sync.incoming.json -pass env:DUKEMEMORY_SYNC_PASSPHRASE"),
        "dukememory sync import memory-sync.incoming.json --policy manual --dry-run --json"
            .to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_sync_plan_never_claims_implicit_execution() {
        let commands = remote_sync_v2_commands(Some("/srv/private-memory"));
        assert!(
            commands
                .iter()
                .any(|command| command.contains("openssl enc"))
        );
        assert!(commands.iter().any(|command| command.contains("--dry-run")));
        assert!(
            commands
                .iter()
                .any(|command| command.contains("/srv/private-memory"))
        );
    }
}

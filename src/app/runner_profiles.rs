use super::*;

const RUNNER_PROFILES_FILE: &str = ".agent/runner-profiles.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunnerProfile {
    pub(crate) runner: String,
    pub(crate) command: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RunnerProfilesConfig {
    #[serde(default)]
    profiles: BTreeMap<String, RunnerProfile>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RunnerProfileStatus {
    pub(crate) name: String,
    #[serde(flatten)]
    pub(crate) profile: RunnerProfile,
    pub(crate) available: bool,
    pub(crate) resolved_command: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunnerProfilesReport {
    version: u32,
    config_path: String,
    profiles: Vec<RunnerProfileStatus>,
    ready: usize,
    unavailable: usize,
}

pub(crate) fn handle_runner_profile(command: RunnerProfileCommand) -> Result<()> {
    match command {
        RunnerProfileCommand::List { root, json } => {
            let report = runner_profiles_report(&root)?;
            print_runner_profiles(&report, json)?;
        }
        RunnerProfileCommand::Doctor { root, json } => {
            let report = runner_profiles_report(&root)?;
            print_runner_profiles(&report, json)?;
            if report.ready == 0 {
                bail!("no configured runner command is available on PATH");
            }
        }
        RunnerProfileCommand::Init { root, apply, json } => {
            let path = root.join(RUNNER_PROFILES_FILE);
            let config = RunnerProfilesConfig {
                profiles: built_in_profiles(),
            };
            let content = toml::to_string_pretty(&config)?;
            if apply {
                if path.exists() {
                    bail!("runner profile config already exists: {}", path.display());
                }
                write_file(&path, content.as_bytes())?;
            }
            let value = json!({
                "version": 1,
                "applied": apply,
                "path": path.display().to_string(),
                "content": content,
            });
            if json {
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else if apply {
                println!("{}", path.display());
            } else {
                println!("preview: {}\n{}", path.display(), content);
            }
        }
    }
    Ok(())
}

pub(crate) fn runner_profiles_status(root: &Path) -> Result<Vec<RunnerProfileStatus>> {
    Ok(runner_profiles_report(root)?.profiles)
}

pub(crate) fn runner_profile_root(db: &Path) -> PathBuf {
    db.parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == ".agent"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn ensure_runner_profile_exists(root: &Path, name: &str) -> Result<()> {
    let profiles = load_runner_profiles(root)?;
    if !profiles.contains_key(name) {
        bail!("unknown runner profile: {name}");
    }
    Ok(())
}

fn runner_profiles_report(root: &Path) -> Result<RunnerProfilesReport> {
    let path = root.join(RUNNER_PROFILES_FILE);
    let profiles = load_runner_profiles(root)?;
    let mut items = profiles
        .into_iter()
        .map(|(name, profile)| {
            let resolved_command = resolve_command(&profile.command);
            RunnerProfileStatus {
                name,
                available: resolved_command.is_some(),
                resolved_command,
                profile,
            }
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    let ready = items.iter().filter(|item| item.available).count();
    Ok(RunnerProfilesReport {
        version: 1,
        config_path: path.display().to_string(),
        unavailable: items.len().saturating_sub(ready),
        ready,
        profiles: items,
    })
}

fn load_runner_profiles(root: &Path) -> Result<BTreeMap<String, RunnerProfile>> {
    let mut profiles = built_in_profiles();
    let path = root.join(RUNNER_PROFILES_FILE);
    if path.exists() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let local: RunnerProfilesConfig =
            toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))?;
        profiles.extend(local.profiles);
    }
    Ok(profiles)
}

fn built_in_profiles() -> BTreeMap<String, RunnerProfile> {
    BTreeMap::from([
        (
            "codex_default".to_string(),
            RunnerProfile {
                runner: "codex".to_string(),
                command: "codex".to_string(),
                model: None,
                effort: None,
                role: "coding".to_string(),
            },
        ),
        (
            "gemini_flash_high".to_string(),
            RunnerProfile {
                runner: "gemini".to_string(),
                command: "gemini".to_string(),
                model: Some("gemini-3.5-flash".to_string()),
                effort: Some("high".to_string()),
                role: "research".to_string(),
            },
        ),
        (
            "antigravity_pro_high".to_string(),
            RunnerProfile {
                runner: "antigravity".to_string(),
                command: "agy".to_string(),
                model: Some("Gemini 3.1 Pro (High)".to_string()),
                effort: Some("high".to_string()),
                role: "review".to_string(),
            },
        ),
        (
            "ollama_local".to_string(),
            RunnerProfile {
                runner: "ollama".to_string(),
                command: "ollama".to_string(),
                model: None,
                effort: None,
                role: "local".to_string(),
            },
        ),
    ])
}

fn resolve_command(command: &str) -> Option<String> {
    let path = Path::new(command);
    if path.components().count() > 1 && path.is_file() {
        return Some(path.display().to_string());
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.display().to_string())
}

fn print_runner_profiles(report: &RunnerProfilesReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        for item in &report.profiles {
            println!(
                "{}  {}  {}  {}",
                item.name,
                if item.available { "ready" } else { "missing" },
                item.profile.command,
                item.profile.role,
            );
        }
    }
    Ok(())
}

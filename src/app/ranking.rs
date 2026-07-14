use super::*;

pub(crate) fn print_ranking_profile(
    root: &Path,
    profile: RankingProfileMode,
    apply: bool,
    json_out: bool,
) -> Result<()> {
    let report = ranking_profile_report(root, profile, apply)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("Ranking Profile");
    println!("profile: {}", report.profile);
    println!("applied: {}", report.applied);
    println!("path: {}", report.path);
    Ok(())
}

pub(crate) fn ranking_profile_report(
    root: &Path,
    profile: RankingProfileMode,
    apply: bool,
) -> Result<RankingProfileReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut weights = BTreeMap::new();
    match profile {
        RankingProfileMode::Balanced => {
            weights.insert("recent_read".to_string(), 0.9);
            weights.insert("useful_feedback".to_string(), 4.0);
            weights.insert("useless_feedback".to_string(), -7.0);
        }
        RankingProfileMode::Strict => {
            weights.insert("recent_read".to_string(), 0.6);
            weights.insert("useful_feedback".to_string(), 3.0);
            weights.insert("useless_feedback".to_string(), -10.0);
        }
        RankingProfileMode::RecallHeavy => {
            weights.insert("recent_read".to_string(), 1.1);
            weights.insert("useful_feedback".to_string(), 3.5);
            weights.insert("useless_feedback".to_string(), -4.0);
        }
        RankingProfileMode::PrecisionHeavy => {
            weights.insert("recent_read".to_string(), 0.7);
            weights.insert("useful_feedback".to_string(), 5.0);
            weights.insert("useless_feedback".to_string(), -12.0);
        }
    }
    let path = root.join(".agent/ranking-profile.json");
    if apply {
        write_file(
            &path,
            serde_json::to_string_pretty(&json!({
                "version": 1,
                "profile": profile.to_string(),
                "weights": &weights,
                "updated_at": now_ms(),
            }))?
            .as_bytes(),
        )?;
    }
    Ok(RankingProfileReport {
        version: 1,
        ok: true,
        root: root.display().to_string(),
        profile: profile.to_string(),
        applied: apply,
        path: path.display().to_string(),
        weights,
        recommendations: vec![
            "profile is resolved once per retrieval from DUKEMEMORY_RANKING_PROFILE or the selected project's .agent/ranking-profile.json".to_string(),
        ],
    })
}

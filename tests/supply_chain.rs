use std::fs;
use std::path::Path;

#[test]
fn github_actions_are_pinned_to_immutable_commit_shas() {
    let workflows = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    for entry in fs::read_dir(workflows).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|value| value.to_str()) != Some("yml") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        for (line_index, line) in source.lines().enumerate() {
            let Some((_, action)) = line.split_once("uses:") else {
                continue;
            };
            let action = action.trim();
            if action.starts_with("./") {
                continue;
            }
            let reference = action
                .split_once('@')
                .unwrap_or_else(|| {
                    panic!("{}:{} action has no ref", path.display(), line_index + 1)
                })
                .1
                .split_whitespace()
                .next()
                .unwrap();
            assert_eq!(
                reference.len(),
                40,
                "{}:{} action ref is not a full commit SHA: {reference}",
                path.display(),
                line_index + 1
            );
            assert!(
                reference.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "{}:{} action ref is not hexadecimal: {reference}",
                path.display(),
                line_index + 1
            );
        }
    }
}

#[test]
fn sbom_generator_and_advisory_exceptions_are_explicitly_pinned() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow = fs::read_to_string(root.join(".github/workflows/security.yml")).unwrap();
    assert!(workflow.contains("cargo-cyclonedx --version 0.5.9 --locked"));
    assert!(workflow.contains("--spec-version 1.5"));
    assert!(workflow.contains("dukememory.cdx.json"));

    let deny = fs::read_to_string(root.join("deny.toml")).unwrap();
    for advisory in ["RUSTSEC-2024-0436", "RUSTSEC-2026-0173"] {
        assert!(deny.contains(advisory));
        assert!(deny.contains("latest upstream release"));
    }
}

#[test]
fn crates_io_publish_uses_short_lived_oidc_after_release_assets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow = fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert!(workflow.contains("crates-io:\n    needs: github-release"));
    assert!(workflow.contains("id-token: write"));
    assert!(workflow.contains("rust-lang/crates-io-auth-action@"));
    assert!(workflow.contains("steps.crates-io-auth.outputs.token"));
    assert!(!workflow.contains("secrets.CARGO_REGISTRY_TOKEN"));
}

#[test]
fn release_evidence_gate_is_reproducible_and_required_by_ci() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let security = fs::read_to_string(root.join(".github/workflows/security.yml")).unwrap();
    let release = fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    let cases = fs::read_to_string(root.join("fixtures/release-evidence/cases.tsv")).unwrap();

    for workflow in [&ci, &security] {
        assert!(workflow.contains("push:\n    branches: [main]"));
        assert!(workflow.contains("pull_request:"));
        assert!(workflow.contains("merge_group:"));
    }
    assert!(ci.contains("release-evidence:"));
    assert!(ci.contains("ci-gate:"));
    assert!(ci.contains("scripts/release-evidence-gate.sh target/debug/dukememory"));
    assert!(security.contains("supply-chain-gate:"));
    assert!(release.contains("scripts/release-evidence-gate.sh target/debug/dukememory"));

    let fixture_cases = cases
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    assert_eq!(fixture_cases.len(), 12);
    assert_eq!(
        fixture_cases
            .iter()
            .filter(|line| line.contains("|holdout|"))
            .count(),
        5
    );
}

#[test]
fn mcp_conformance_claim_is_versioned_and_explicitly_scoped() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let profile: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("mcp-conformance-profile.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(profile["suite_version"], "0.1.16");
    assert_eq!(profile["protocol_version"], "2026-07-28");
    assert!(
        profile["generic_scenarios"]
            .as_array()
            .is_some_and(|items| items.len() >= 5)
    );
    assert!(
        profile["fixture_bound_scenarios_not_claimed"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
}

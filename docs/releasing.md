# Releasing dukememory

Releases are tag-driven. A tag such as `v0.43.0` must exactly match the package
version in `Cargo.toml` and `Cargo.lock`.

## One-time repository setup

Create a protected GitHub environment named `crates-io`, configure crates.io as
a trusted publisher for this repository/workflow, and use environment reviewers
if publication requires manual approval. The workflow requests a short-lived
OIDC token; no long-lived `CARGO_REGISTRY_TOKEN` secret is stored.

The release workflow requests read-only repository access by default. Only the
GitHub release job receives `contents: write`; the crates.io job receives only
`id-token: write` inside the protected environment.

## Release sequence

1. Update `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and user-facing docs.
2. Run the local gate:

   ```bash
   dukememory release-gate-v3 --profile code --strict --json
   dukememory release-gate-v3 --profile project --json
   # Run this profile only for the runtime configuration being deployed.
   dukememory release-gate-v3 --profile deployment --json
   cargo fmt --all -- --check
   scripts/dependency-budget.sh
   cargo clippy --locked --all-targets --all-features -- -D warnings
   cargo test --locked
   cargo test --locked --features vec
   cargo test --locked --test performance -- --ignored --nocapture
   cargo deny check advisories bans licenses sources
   cargo cyclonedx --format json --all-features --target all \
     --spec-version 1.5 --override-filename dukememory.cdx
   jq -e '.bomFormat == "CycloneDX" and (.components | length > 0)' \
     dukememory.cdx.json
   cargo package --locked
   cargo build --locked --release --features vec
   scripts/binary-size-budget.sh target/release/dukememory 57671680
   cargo build --locked --profile release-minimal --no-default-features --features vec
   scripts/binary-size-budget.sh target/release-minimal/dukememory 12582912
   scripts/release-smoke.sh target/release/dukememory 0.43.0
   scripts/reproducible-build-check.sh
   ```

   A local-only deployment does not require a remote sync target. When
   `DUKEMEMORY_SYNC_TARGET` is set, sync latency and the local-first backup
   profile become required deployment checks; without it they remain visible
   but advisory.

   Release builds use stripped symbols, thin LTO, one codegen unit, disabled
   incremental compilation, and `panic = "abort"`. The reproducibility check
   builds the FTS/external-embedding profile twice from clean state in one
   disposable remapped target path with a fixed `SOURCE_DATE_EPOCH`, then
   requires byte-identical binaries. Keeping the build path stable avoids
   platform-native dependencies treating the test harness path as an input.
   The tag workflow publishes that minimal Linux x86_64 artifact separately;
   its enforced ceiling is 12 MiB (the measured macOS build is about 7.5 MiB).

3. Merge the reviewed release commit to `main` and create the signed or
   annotated tag `v0.43.0` on that commit.
4. Push the tag. `.github/workflows/release.yml` verifies the version, package,
   formatting, Clippy, tests, the performance gate, dependency/size policy, and the
   CycloneDX SBOM; builds Linux x86_64 GNU, Linux ARM64 GNU, Linux x86_64 musl,
   macOS arm64/x86_64, and Windows x86_64 archives; smoke-tests an installed
   copy; emits the SBOM plus per-archive and
   combined SHA-256 manifests; creates the GitHub release; and publishes the
   crate with `cargo publish --locked`.
   Separate pinned CodeQL, mutation, fuzz, and coverage workflows provide the
   scheduled and pull-request companion gates.
5. Verify the GitHub assets and `SHA256SUMS`, then confirm the version on
   crates.io. Never move or reuse a released version tag.

If trusted publishing is not configured, the OIDC exchange fails closed.
Configure the crates.io trusted publisher and rerun that job; do not paste a
token into logs or CLI arguments.

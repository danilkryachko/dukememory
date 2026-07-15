# Releasing dukememory

Releases are tag-driven. A tag such as `v0.43.0` must exactly match the package
version in `Cargo.toml` and `Cargo.lock`.

## One-time repository setup

Create a protected GitHub environment named `crates-io`. Add a scoped crates.io
publishing token as the environment secret `CARGO_REGISTRY_TOKEN`; do not store
the token in the repository or on a release machine. Limit the token to the
`dukememory` crate and use environment reviewers if the repository requires a
manual publication approval.

The release workflow requests read-only repository access by default. Only the
GitHub release job receives `contents: write`; the crates.io job receives the
publishing token only inside the protected environment.

## Release sequence

1. Update `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and user-facing docs.
2. Run the local gate:

   ```bash
   cargo fmt --all -- --check
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
   scripts/release-smoke.sh target/release/dukememory 0.43.0
   ```

3. Merge the reviewed release commit to `main` and create the signed or
   annotated tag `v0.43.0` on that commit.
4. Push the tag. `.github/workflows/release.yml` verifies the version, package,
   formatting, Clippy, tests, the performance gate, dependency policy, and the
   CycloneDX SBOM; builds Linux x86_64 GNU, Linux ARM64 GNU, Linux x86_64 musl,
   macOS arm64/x86_64, and Windows x86_64 archives; smoke-tests an installed
   copy; emits the SBOM plus per-archive and
   combined SHA-256 manifests; creates the GitHub release; and publishes the
   crate with `cargo publish --locked`.
5. Verify the GitHub assets and `SHA256SUMS`, then confirm the version on
   crates.io. Never move or reuse a released version tag.

If the crates.io secret is absent, the publishing job fails closed. Configure
the protected secret and rerun that job; do not paste a token into logs or CLI
arguments.

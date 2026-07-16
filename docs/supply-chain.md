# Supply-chain policy

All third-party GitHub Actions are pinned to immutable 40-character commit SHAs. `tests/supply_chain.rs` rejects mutable workflow references.

CI runs `cargo-deny` for advisories, licenses, bans, and sources. The dependency graph currently has two reviewed unmaintained build-time transitive exceptions:

- `RUSTSEC-2024-0436`: `tokenizers 0.23.1 → paste`. `tokenizers` is required only by the optional `local-embeddings` feature. Remove the exception when upstream replaces `paste`.
- `RUSTSEC-2026-0173`: `age 0.12.1 → i18n-embed-fl → proc-macro-error2`. `age` provides encrypted sync bundles. Remove the exception when upstream replaces `proc-macro-error2`.

Both are pinned by `Cargo.lock`, are not runtime parsing or network entry points, and must remain visible in `deny.toml`; new advisory exceptions require their own rationale.

The supply-chain workflow installs the pinned `cargo-cyclonedx 0.5.9` release and generates a CycloneDX 1.5 JSON SBOM:

```bash
cargo install cargo-cyclonedx --version 0.5.9 --locked
cargo cyclonedx --format json --all-features --target all \
  --spec-version 1.5 --override-filename dukememory.cdx
jq -e '.bomFormat == "CycloneDX" and (.components | length > 0)' dukememory.cdx.json
```

Tagged releases also generate GitHub/Sigstore build-provenance attestations for
every archive, checksum, the combined `SHA256SUMS`, and the CycloneDX SBOM. The
attestation action is pinned to the reviewed immutable SHA for `actions/attest
v4.1.1`. After downloading an asset, verify both its checksum and provenance:

```bash
sha256sum --check SHA256SUMS --ignore-missing
gh attestation verify dukememory-x86_64-unknown-linux-gnu.tar.gz \
  --repo danilkryachko/dukememory
```

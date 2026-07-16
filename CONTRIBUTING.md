# Contributing to DukeMemory

Thank you for improving DukeMemory. Keep changes focused, local-first, and
compatible with the stable operation catalog.

## Development setup

The declared minimum toolchain is Rust 1.91. From the repository root:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
```

See `docs/testing.md` for coverage, fuzzing, MCP conformance, mutation, and
release-gate commands.

## Pull requests

- Open a focused branch and pull request; direct updates to `main` are blocked.
- Add regression coverage for behavior changes and security boundaries.
- Keep mutations dry-run-first and reversible unless the interface explicitly
  requires an apply operation.
- Update the operation catalog and generated documentation when a stable CLI,
  MCP, or HTTP contract changes.
- Do not commit credentials, local `.agent` data, model artifacts, build output,
  or generated release archives.
- Do not create tags, releases, or registry publications from a contribution.

The required GitHub checks must pass before merge. Security-sensitive changes
to authentication, egress, RAG ingestion, audit integrity, workflows, or the
release pipeline require explicit maintainer review.

## Compatibility and deprecation

Prefer the unversioned stable surfaces. Compatibility aliases may remain hidden
for existing clients, but new version-suffixed commands should not be added.
Breaking changes require a documented migration path and release note.

## Security reports

Follow `SECURITY.md`; never disclose an unpatched vulnerability in a public
issue or pull request.

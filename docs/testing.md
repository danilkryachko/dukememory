# Testing and fuzzing

The normal quality gate uses the pinned lockfile:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --features vec
```

CI also generates source-based coverage with `cargo-llvm-cov` 0.8.6 and
enforces a 55% repository-wide line floor. The current suite has measured above
60%; the lower enforced floor leaves bounded platform variance while preventing
the former 30% threshold from masking a large regression.
`scripts/coverage-budget.sh` additionally enforces higher per-file floors for
protocol, SQLite, audit, auth, MCP transport/server, RAG security, and ingest.
The quality job rejects growth past the checked-in dependency and duplicate
version budgets. CodeQL analyzes compiled Rust on pushes, pull requests, and a
weekly schedule; a separate weekly/manual cargo-mutants job gates bounded
critical security modules.

The protocol framing parsers are exposed from the small `dukememory::protocol`
library boundary so unit/property tests and fuzz targets exercise the same code
used by the HTTP and MCP adapters. The pure pre-retrieval quarantine detector is
similarly exposed as `dukememory::rag_security`, keeping its fuzz target free of
database and model-provider state.

Fuzzing requires nightly Rust and `cargo-fuzz` 0.12.0:

```bash
cargo install cargo-fuzz --version 0.12.0 --locked
cargo fuzz run mcp_content_length -- -max_total_time=60
cargo fuzz run http_framing -- -max_total_time=60
cargo fuzz run rag_prompt_injection -- -max_total_time=60
cargo fuzz run sync_payload -- -max_total_time=60
cargo fuzz run egress_url -- -max_total_time=60
```

Pull requests touching protocol framing get a bounded 30-second smoke run per
target. The scheduled workflow runs each target for five minutes and uploads
crash artifacts on failure.

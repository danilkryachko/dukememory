# Testing and fuzzing

The normal quality gate uses the pinned lockfile:

```bash
cargo fmt --all -- --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --features vec
```

CI also generates source-based coverage with `cargo-llvm-cov` 0.8.6 and
enforces a 30% repository-wide line floor. This is an initial regression floor,
not a claim that 30% is sufficient; raise it only after the checked-in suite
reliably exceeds the new threshold.

The protocol framing parsers are exposed from the small `dukememory::protocol`
library boundary so unit/property tests and fuzz targets exercise the same code
used by the HTTP and MCP adapters.

Fuzzing requires nightly Rust and `cargo-fuzz` 0.12.0:

```bash
cargo install cargo-fuzz --version 0.12.0 --locked
cargo fuzz run mcp_content_length -- -max_total_time=60
cargo fuzz run http_framing -- -max_total_time=60
```

Pull requests touching protocol framing get a bounded 30-second smoke run per
target. The scheduled workflow runs each target for five minutes and uploads
crash artifacts on failure.

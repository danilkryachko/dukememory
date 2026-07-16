#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
profile=${DUKEMEMORY_REPRO_PROFILE:-release-minimal}
features=${DUKEMEMORY_REPRO_FEATURES:-vec}
source_date_epoch=${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

export SOURCE_DATE_EPOCH="$source_date_epoch"
export CARGO_INCREMENTAL=0
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$root=/src/dukememory"

build_once() {
  CARGO_TARGET_DIR="$workdir/target" cargo build \
    --manifest-path "$root/Cargo.toml" \
    --locked \
    --profile "$profile" \
    --no-default-features \
    --features "$features"
}

binary=dukememory
if [[ "${OS:-}" == "Windows_NT" ]]; then
  binary=dukememory.exe
fi
first="$workdir/first-$binary"
second="$workdir/second-$binary"

build_once
cp "$workdir/target/$profile/$binary" "$first"
cargo clean --manifest-path "$root/Cargo.toml" --target-dir "$workdir/target"
build_once
cp "$workdir/target/$profile/$binary" "$second"

if ! cmp -s "$first" "$second"; then
  echo "reproducible build check failed: binaries differ" >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  digest=$(sha256sum "$first" | awk '{print $1}')
else
  digest=$(shasum -a 256 "$first" | awk '{print $1}')
fi
bytes=$(wc -c <"$first" | tr -d ' ')
echo "reproducible build: ok profile=$profile bytes=$bytes sha256=$digest"

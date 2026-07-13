#!/usr/bin/env bash
set -euo pipefail

source_binary=${1:?usage: release-smoke.sh BINARY [EXPECTED_VERSION]}
expected_version=${2:-}

if [[ ! -x "$source_binary" ]]; then
  echo "release binary is not executable: $source_binary" >&2
  exit 1
fi

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

installed_binary="$workdir/install/dukememory"
database="$workdir/project/.agent/memory.db"
sync_target="$workdir/remote"
mkdir -p "$(dirname "$installed_binary")" "$(dirname "$database")"

"$source_binary" --db "$database" update-install \
  --from "$source_binary" \
  --to "$installed_binary" \
  --backup-dir "$workdir/install-backups" \
  --json >"$workdir/install.json"

version_output=$("$installed_binary" --version)
if [[ -n "$expected_version" && "$version_output" != "dukememory $expected_version" ]]; then
  echo "unexpected installed version: $version_output" >&2
  exit 1
fi

"$installed_binary" --db "$database" schema verify >"$workdir/schema.txt"
memory_id=$(
  "$installed_binary" --db "$database" add decision \
    "Release smoke memory" \
    "Installed release binary can create, retrieve, and sync durable memory."
)
"$installed_binary" --db "$database" get "$memory_id" >"$workdir/get.txt"
"$installed_binary" --db "$database" embed-index \
  --provider mock \
  --endpoint local \
  --model mock-small >"$workdir/embed.json"
"$installed_binary" --db "$database" embed-search \
  "release smoke memory" \
  --provider mock \
  --endpoint local \
  --model mock-small \
  --backend sqlite-vec \
  --json >"$workdir/search.json"
"$installed_binary" --db "$database" vec-validate \
  --backend sqlite-vec >"$workdir/vec.txt"
"$installed_binary" --db "$database" vec-index --json >"$workdir/vec-index.json"
"$installed_binary" --db "$database" sync push "$sync_target" \
  --json >"$workdir/push.json"
"$installed_binary" --db "$database" sync status "$sync_target" \
  --json >"$workdir/status.json"
"$installed_binary" --db "$database" integrity --json >"$workdir/integrity.json"

grep -q '"consistent": true' "$workdir/vec-index.json"
grep -q '"generation":' "$workdir/push.json"
grep -q '"verified": true' "$workdir/status.json"
grep -q '"integrity_check": "ok"' "$workdir/integrity.json"

echo "release smoke: ok ($version_output)"

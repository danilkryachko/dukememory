#!/usr/bin/env bash
set -euo pipefail

BINARY="${1:-target/debug/dukememory}"
HOST="${DUKEMEMORY_MCP_CONFORMANCE_HOST:-127.0.0.1}"
PORT="${DUKEMEMORY_MCP_CONFORMANCE_PORT:-18765}"
PROFILE="${DUKEMEMORY_MCP_CONFORMANCE_PROFILE:-mcp-conformance-profile.json}"
test -f "${PROFILE}" || {
  printf 'MCP conformance profile not found: %s\n' "${PROFILE}" >&2
  exit 1
}
PROFILE_VERSION="$(jq -r '.suite_version' "${PROFILE}")"
CONFORMANCE_VERSION="${MCP_CONFORMANCE_VERSION:-${PROFILE_VERSION}}"
if [[ "${CONFORMANCE_VERSION}" != "${PROFILE_VERSION}" ]]; then
  printf 'MCP suite override %s does not match reviewed profile %s\n' \
    "${CONFORMANCE_VERSION}" "${PROFILE_VERSION}" >&2
  exit 1
fi
SCENARIOS=()
while IFS= read -r scenario; do
  SCENARIOS+=("${scenario}")
done < <(jq -er '.generic_scenarios[]' "${PROFILE}")
if (( ${#SCENARIOS[@]} < 5 )); then
  printf 'MCP conformance profile must retain at least five generic scenarios\n' >&2
  exit 1
fi
DATABASE="$(mktemp -t dukememory-mcp-conformance.XXXXXX.db)"
SERVER_LOG="$(mktemp -t dukememory-mcp-conformance.XXXXXX.log)"
SERVER_PID=""

cleanup() {
  if [[ -n "${SERVER_PID}" ]]; then
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  rm -f "${DATABASE}" "${DATABASE}-shm" "${DATABASE}-wal" "${SERVER_LOG}"
}
trap cleanup EXIT

"${BINARY}" \
  --db "${DATABASE}" \
  serve-http \
  --host "${HOST}" \
  --port "${PORT}" \
  --mcp-profile core \
  >"${SERVER_LOG}" 2>&1 &
SERVER_PID=$!

for _ in {1..60}; do
  if curl --fail --silent "http://${HOST}:${PORT}/health" >/dev/null; then
    break
  fi
  if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
    cat "${SERVER_LOG}" >&2
    exit 1
  fi
  sleep 0.25
done
curl --fail --silent "http://${HOST}:${PORT}/health" >/dev/null

for scenario in "${SCENARIOS[@]}"; do
  printf 'MCP conformance %s@%s scenario=%s\n' \
    "@modelcontextprotocol/conformance" "${CONFORMANCE_VERSION}" "${scenario}"
  npx -y "@modelcontextprotocol/conformance@${CONFORMANCE_VERSION}" \
    server \
    --url "http://${HOST}:${PORT}/mcp" \
    --scenario "${scenario}"
done

printf 'MCP generic profile passed (%s scenarios); fixture-bound scenarios remain explicitly unclaimed.\n' \
  "${#SCENARIOS[@]}"

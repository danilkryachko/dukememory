#!/usr/bin/env bash
set -euo pipefail

check_lines() {
  local path="$1"
  local maximum="$2"
  local lines
  lines="$(wc -l <"${path}" | tr -d ' ')"
  if (( lines > maximum )); then
    printf '%s has %s lines; architecture budget is %s. Extract a cohesive module before adding more.\n' \
      "${path}" "${lines}" "${maximum}" >&2
    return 1
  fi
  printf '%s: %s/%s lines\n' "${path}" "${lines}" "${maximum}"
}

check_lines src/app/observability.rs 19200
check_lines src/app/observability/rag_eval_summary.rs 400
check_lines src/app/diagnostics.rs 5400
check_lines tests/cli.rs 17500
check_lines src/app/mcp_server.rs 4200
check_lines src/app/http_routes.rs 3700
check_lines src/app.rs 3975
check_lines src/app/cli.rs 3425
check_lines src/app/dispatch.rs 2325

#!/usr/bin/env bash
set -euo pipefail

lcov_file="${1:-lcov.info}"
test -f "${lcov_file}" || {
  printf 'coverage report not found: %s\n' "${lcov_file}" >&2
  exit 1
}

check_file() {
  local suffix="$1"
  local minimum="$2"
  local totals
  totals="$({
    awk -v suffix="${suffix}" '
      /^SF:/ {
        file = substr($0, 4)
        active = length(file) >= length(suffix) && substr(file, length(file) - length(suffix) + 1) == suffix
      }
      active && /^LF:/ { lf = substr($0, 4) }
      active && /^LH:/ { lh = substr($0, 4); print lh, lf; exit }
    ' "${lcov_file}"
  })"
  if [[ -z "${totals}" ]]; then
    printf 'critical coverage file missing from LCOV: %s\n' "${suffix}" >&2
    return 1
  fi
  local hit total
  read -r hit total <<<"${totals}"
  if (( total <= 0 || hit * 100 < total * minimum )); then
    printf '%s coverage %s/%s is below %s%%\n' "${suffix}" "${hit}" "${total}" "${minimum}" >&2
    return 1
  fi
  awk -v file="${suffix}" -v hit="${hit}" -v total="${total}" -v minimum="${minimum}" \
    'BEGIN { printf "%s: %d/%d (%.1f%%, floor %d%%)\n", file, hit, total, 100 * hit / total, minimum }'
}

check_file src/protocol.rs 85
check_file src/rag_security.rs 80
check_file src/app/db.rs 85
check_file src/app/shared.rs 85
check_file src/app/http_security.rs 70
check_file src/app/http_authorization.rs 85
check_file src/app/http_routes.rs 20
check_file src/app/mcp_transport.rs 85
check_file src/app/mcp_server.rs 60
check_file src/app/rag_ingest.rs 70

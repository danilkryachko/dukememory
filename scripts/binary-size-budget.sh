#!/usr/bin/env bash
set -euo pipefail

binary="${1:?usage: binary-size-budget.sh BINARY [MAX_BYTES]}"
maximum="${2:-57671680}"
test -f "${binary}" || {
  printf 'binary not found: %s\n' "${binary}" >&2
  exit 1
}

case "$(uname -s)" in
  Darwin|FreeBSD) bytes="$(stat -f%z "${binary}")" ;;
  *) bytes="$(stat -c%s "${binary}")" ;;
esac
printf '%s: %s/%s bytes\n' "${binary}" "${bytes}" "${maximum}"
(( bytes <= maximum )) || {
  printf 'binary size budget exceeded\n' >&2
  exit 1
}

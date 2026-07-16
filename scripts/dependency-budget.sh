#!/usr/bin/env bash
set -euo pipefail

maximum_packages="${DUKEMEMORY_MAX_LOCKED_PACKAGES:-480}"
maximum_duplicate_families="${DUKEMEMORY_MAX_DUPLICATE_FAMILIES:-20}"

packages="$(awk '/^\[\[package\]\]$/ { count++ } END { print count + 0 }' Cargo.lock)"
duplicates="$({
  cargo tree --locked --duplicates --depth 0 |
    awk 'NF { print $1, $2 }' |
    sort -u |
    awk '{ versions[$1]++ } END { for (name in versions) if (versions[name] > 1) count++; print count + 0 }'
})"

printf 'locked packages: %s/%s\n' "${packages}" "${maximum_packages}"
printf 'duplicate-version families: %s/%s\n' "${duplicates}" "${maximum_duplicate_families}"

(( packages <= maximum_packages )) || {
  printf 'dependency package budget exceeded\n' >&2
  exit 1
}
(( duplicates <= maximum_duplicate_families )) || {
  printf 'duplicate dependency budget exceeded; inspect cargo tree --duplicates\n' >&2
  exit 1
}

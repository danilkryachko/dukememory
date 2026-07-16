#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${1:-${repo_root}/target/debug/dukememory}"
if [[ "${binary}" != /* ]]; then
  binary="${repo_root}/${binary}"
fi
test -x "${binary}" || {
  printf 'release evidence binary is not executable: %s\n' "${binary}" >&2
  exit 1
}

fixture_root="${repo_root}/fixtures/release-evidence"
test -f "${fixture_root}/source.md" || {
  printf 'release evidence source is missing\n' >&2
  exit 1
}
test -f "${fixture_root}/cases.tsv" || {
  printf 'release evidence cases are missing\n' >&2
  exit 1
}

work_root="$(mktemp -d -t dukememory-release-evidence.XXXXXX)"
trap 'rm -rf "${work_root}"' EXIT
mkdir -p "${work_root}/evidence"
cp "${fixture_root}/source.md" "${work_root}/evidence/source.md"

export DUKEMEMORY_EMBED_PROVIDER=mock
export DUKEMEMORY_EMBED_ENDPOINT=local
export DUKEMEMORY_EMBED_MODEL=mock-small
export DUKEMEMORY_GEN_PROVIDER=mock

"${binary}" onboard \
  --root "${work_root}" \
  --provider mock \
  --endpoint local \
  --model mock-small \
  --json >/dev/null

while IFS='|' read -r id split name query expected; do
  [[ -z "${id}" || "${id}" == \#* ]] && continue
  (
    cd "${work_root}"
    "${binary}" add design_note "${name}" \
      "Synthetic release evidence marker ${expected}. This card is part of the reproducible CI corpus." \
      --id "${id}" \
      --source "fixture:release-evidence-v1" >/dev/null
    "${binary}" eval add-case "${name}" "${query}" "${expected}" \
      --split "${split}" \
      --budget 3000 >/dev/null
  )
done <"${fixture_root}/cases.tsv"

(
  cd "${work_root}"
  "${binary}" observe releaseev01 \
    --kind causes \
    --statement "The reviewed CLI evidence causes the MCP workflow requirement" \
    --evidence-kind test \
    --evidence-ref "release-evidence:causal-01" \
    --target-memory-id releaseev02 \
    --confidence 1.0 \
    --root "${work_root}" \
    --json >/dev/null
  "${binary}" observe releaseev02 \
    --kind enables \
    --statement "The reviewed MCP evidence enables the HTTP workflow requirement" \
    --evidence-kind test \
    --evidence-ref "release-evidence:causal-02" \
    --target-memory-id releaseev03 \
    --confidence 1.0 \
    --root "${work_root}" \
    --json >/dev/null
)

(
  cd "${work_root}"
  "${binary}" rag-ingest "${work_root}/evidence/source.md" \
    --root "${work_root}" \
    --apply \
    --reviewed \
    --embed \
    --provider mock \
    --endpoint local \
    --model mock-small \
    --json >/dev/null
)

(
  cd "${work_root}"
  "${binary}" embed-index \
    --provider mock \
    --endpoint local \
    --model mock-small >/dev/null
  "${binary}" eval rag \
    --limit 8 \
    --budget 3000 \
    --provider mock \
    --endpoint local \
    --model mock-small \
    --write-baseline \
    --json >"${work_root}/rag-eval.json"
  "${binary}" release-gate-v3 \
    --root "${work_root}" \
    --profile deployment \
    --rag-profile offline \
    --json >"${work_root}/release-gate.json"
  "${binary}" eval advanced --json >"${work_root}/advanced-eval.json"
)

jq '{
  status,
  selected_profile,
  rag_cases: .rag_eval.total,
  holdout_cases: .rag_eval.split.holdout_total,
  hit_at_3: .rag_eval.ranking.hit_at_3_rate,
  baseline: .rag_eval.baseline.status,
  rag_sources: (
    .checks[] |
    select(.name == "rag_sources_freshness") |
    {ok, detail}
  ),
  failed_required: [
    .profiles[] |
    select(.name == "deployment") |
    .failed_required_checks[]
  ]
}' "${work_root}/release-gate.json"

jq -e '
  .poisoning.memory_provenance_coverage >= 80 and
  .poisoning.generated_output_guard_passed == .poisoning.generated_output_guard_total and
  .poisoning.generated_output_false_accepts == 0 and
  .causal.causal_edges >= 2 and
  .causal.evidence_coverage == 100 and
  .temporal.status == "ready"
' "${work_root}/advanced-eval.json" >/dev/null

jq '{
  advanced_status: .status,
  memory_provenance: .poisoning.memory_provenance_coverage,
  generated_output_guard: "\(.poisoning.generated_output_guard_passed)/\(.poisoning.generated_output_guard_total)",
  causal_edges: .causal.causal_edges,
  causal_evidence_coverage: .causal.evidence_coverage,
  temporal_status: .temporal.status
}' "${work_root}/advanced-eval.json"

jq -e '
  .ok == true and
  .selected_profile == "deployment" and
  ([.profiles[] | select(.name == "deployment") | .failed_required_checks[]] | length) == 0 and
  .rag_eval.case_source == "stored" and
  .rag_eval.eval_matrix.stored_cases >= 12 and
  .rag_eval.split.holdout_ready == true and
  .rag_eval.baseline.status == "matched"
' "${work_root}/release-gate.json" >/dev/null

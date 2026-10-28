#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/action/publication.sh
source "${BASH_SOURCE[0]%/*}/publication.sh"
: "${GITHUB_OUTPUT:?output file is required}"
output ready false
output release-commit ''
output releases '[]'
output publication-plan '[]'
setup_readonly
verify_publication_batch
if [[ -z $release ]]; then
  output ready true
  printf 'No release batch to publish.\n'
  exit 0
fi
inspect_publication
output release-commit "$release"
output releases "$(jq -c 'map({package, version, tag})' "$temp/batch.json")"
output publication-plan "$(jq -c . "$temp/publication-plan.json")"
output ready "$ready"
printf 'Release commit: %s\n' "$release"
jq -r '.[] | "\(.name)@\(.version): \(.action)", (.conflicts[] | "  " + .)' "$temp/publication-plan.json"
require_publication_ready

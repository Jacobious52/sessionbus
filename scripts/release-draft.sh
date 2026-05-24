#!/usr/bin/env bash
set -euo pipefail

tag="${1:-v0.1.0}"
notes_file="${NOTES_FILE:-}"
dry_run="${DRY_RUN:-0}"

if [[ -z "$notes_file" ]]; then
  notes_file="$(mktemp -t sessionbus-release-notes.XXXXXX.md)"
  bun run scripts/release-notes.ts "$tag" > "$notes_file"
fi

cmd=(gh release create "$tag" --draft --title "Sessionbus $tag" --notes-file "$notes_file")

printf '+'
for arg in "${cmd[@]}"; do
  printf ' %q' "$arg"
done
printf '\n'

if [[ "$dry_run" == "1" ]]; then
  echo "DRY_RUN=1; release draft not created."
  echo "notes_file=$notes_file"
  exit 0
fi

"${cmd[@]}"

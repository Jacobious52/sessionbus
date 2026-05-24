#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix="${PREFIX:-$HOME/.local}"
profile="${PROFILE:-release}"
bin_dir="$prefix/bin"
dry_run="${DRY_RUN:-0}"

run() {
  printf '+ %q' "$1"
  for arg in "${@:2}"; do
    printf ' %q' "$arg"
  done
  printf '\n'
  if [[ "$dry_run" != "1" ]]; then
    "$@"
  fi
}

if [[ "$dry_run" != "1" ]]; then
  mkdir -p "$bin_dir"
fi

if [[ "$profile" == "debug" ]]; then
  run cargo install --path "$repo_root/crates/aictx-cli" --bin aictx --root "$prefix" --debug
else
  run cargo install --path "$repo_root/crates/aictx-cli" --bin aictx --root "$prefix"
fi

cat <<EOF

Sessionbus installed:
  $bin_dir/aictx

Next:
  export PATH="$bin_dir:\$PATH"
  aictx setup

Optional local install:
  aictx setup --write --auto-capture --open-dashboard
EOF

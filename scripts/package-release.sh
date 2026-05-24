#!/usr/bin/env bash
set -euo pipefail

version="${VERSION:-${1:-v0.1.0}}"
profile="${PROFILE:-release}"
dist_dir="${DIST_DIR:-dist}"
skip_build="${SKIP_BUILD:-0}"

case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux) os="unknown-linux-gnu" ;;
  *) os="$(uname -s | tr '[:upper:]' '[:lower:]')" ;;
esac

case "$(uname -m)" in
  arm64|aarch64) arch="aarch64" ;;
  x86_64|amd64) arch="x86_64" ;;
  *) arch="$(uname -m)" ;;
esac

target_dir="target/$profile"
archive_name="sessionbus-${version}-${arch}-${os}"
staging="$dist_dir/$archive_name"

if [[ "$skip_build" != "1" ]]; then
  if [[ "$profile" == "debug" ]]; then
    cargo build --workspace
  else
    cargo build --workspace --release
  fi
fi

rm -rf "$staging"
mkdir -p "$staging/bin" "$dist_dir"

cp "$target_dir/aictx" "$staging/bin/aictx"
cp "$target_dir/sessionbus-acp-bridge" "$staging/bin/sessionbus-acp-bridge"
cp README.md "$staging/README.md"
cp LICENSE-APACHE "$staging/LICENSE-APACHE"
cp LICENSE-MIT "$staging/LICENSE-MIT"
cp docs/release.md "$staging/RELEASE.md"

tarball="$dist_dir/$archive_name.tar.gz"
tar -C "$dist_dir" -czf "$tarball" "$archive_name"

if command -v shasum >/dev/null 2>&1; then
  (cd "$dist_dir" && shasum -a 256 "$(basename "$tarball")" > "$(basename "$tarball").sha256")
else
  (cd "$dist_dir" && sha256sum "$(basename "$tarball")" > "$(basename "$tarball").sha256")
fi

echo "$tarball"
echo "$tarball.sha256"

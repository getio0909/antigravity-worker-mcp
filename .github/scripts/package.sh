#!/usr/bin/env bash
set -euo pipefail

version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
target=${RELEASE_TARGET:?RELEASE_TARGET is required}
archive="antigravity-worker-mcp-v${version}-${target}.tar.gz"
mkdir -p release artifacts
cp "target/${target}/release/antigravity-worker-mcp" release/
cp LICENSE THIRD_PARTY_NOTICES.md README.md CONTRIBUTING.md SECURITY.md CHANGELOG.md release/
cp -R licenses docs examples release/
tar -C release -czf "artifacts/${archive}" .
if command -v sha256sum >/dev/null 2>&1; then
  (cd artifacts && sha256sum "$archive" > "$archive.sha256")
else
  (cd artifacts && shasum -a 256 "$archive" > "$archive.sha256")
fi
"release/antigravity-worker-mcp" --version

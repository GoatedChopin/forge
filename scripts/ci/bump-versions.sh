#!/usr/bin/env bash
# Usage: bump-versions.sh <version>
set -euo pipefail

VERSION="$1"
echo "Bumping to $VERSION"

cargo set-version --workspace "$VERSION"

for crate_dir in crates/forge crates/forge-runtime crates/forge-codegen crates/forge-macros crates/forge-core; do
  [ -f "$crate_dir/Cargo.toml" ] || continue
  sed -i "s/forge-core = { version = \"[^\"]*\"/forge-core = { version = \"$VERSION\"/g" "$crate_dir/Cargo.toml"
  sed -i "s/forge-macros = { version = \"[^\"]*\"/forge-macros = { version = \"$VERSION\"/g" "$crate_dir/Cargo.toml"
  sed -i "s/forge-runtime = { version = \"[^\"]*\"/forge-runtime = { version = \"$VERSION\"/g" "$crate_dir/Cargo.toml"
  sed -i "s/forge-codegen = { version = \"[^\"]*\"/forge-codegen = { version = \"$VERSION\"/g" "$crate_dir/Cargo.toml"
done

# Runtime packages
[ -f packages/forge-svelte/package.json ] && \
  jq --arg v "$VERSION" '.version = $v' packages/forge-svelte/package.json > packages/forge-svelte/package.json.tmp && \
  mv packages/forge-svelte/package.json.tmp packages/forge-svelte/package.json
[ -f packages/forge-dioxus/Cargo.toml ] && \
  sed -i "s/^version = \"[^\"]*\"/version = \"$VERSION\"/" packages/forge-dioxus/Cargo.toml

# Examples use workspace/path deps and stay linked to source.
# build-template-archive.sh rewrites them to published versions at archive time.

# Docs
[ -f docs/package.json ] && \
  jq --arg v "$VERSION" '.version = $v' docs/package.json > docs/package.json.tmp && \
  mv docs/package.json.tmp docs/package.json
find docs -name "*.mdx" -o -name "*.md" | xargs -I {} sed -i "s/forge = { version = \"[^\"]*\"/forge = { version = \"$VERSION\"/g" {} 2>/dev/null || true
find docs -name "*.mdx" -o -name "*.md" | xargs -I {} sed -i "s/forgex = { version = \"[^\"]*\"/forgex = { version = \"$VERSION\"/g" {} 2>/dev/null || true

echo "Done. Verify with: git diff --stat"

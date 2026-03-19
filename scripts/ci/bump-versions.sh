#!/usr/bin/env bash
# Usage: bump-versions.sh <version> <forge-binary>
set -euo pipefail

VERSION="$1"
FORGE="$2"
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

# Example deps
for pkg in examples/with-*/*/frontend/package.json; do
  [ -f "$pkg" ] || continue
  jq --arg v "=$VERSION" '
    if .dependencies["@forge-rs/svelte"] then .dependencies["@forge-rs/svelte"] = $v
    elif .devDependencies["@forge-rs/svelte"] then .devDependencies["@forge-rs/svelte"] = $v
    else . end
  ' "$pkg" > "$pkg.tmp" && mv "$pkg.tmp" "$pkg"
done
for cargo in examples/with-*/*/frontend/Cargo.toml; do
  [ -f "$cargo" ] && sed -i "s/forge-dioxus\", version = \"=[^\"]*\"/forge-dioxus\", version = \"=$VERSION\"/g" "$cargo"
done
for cargo_toml in examples/with-*/*/Cargo.toml; do
  [ -f "$cargo_toml" ] && sed -i "s/forge = { \(version\|path\) = \"[^\"]*\"/forge = { version = \"$VERSION\"/g" "$cargo_toml"
done

# Docs
[ -f docs/package.json ] && \
  jq --arg v "$VERSION" '.version = $v' docs/package.json > docs/package.json.tmp && \
  mv docs/package.json.tmp docs/package.json
find docs -name "*.mdx" -o -name "*.md" | xargs -I {} sed -i "s/forge = { version = \"[^\"]*\"/forge = { version = \"$VERSION\"/g" {} 2>/dev/null || true
find docs -name "*.mdx" -o -name "*.md" | xargs -I {} sed -i "s/forgex = { version = \"[^\"]*\"/forgex = { version = \"$VERSION\"/g" {} 2>/dev/null || true

# Create frontend .env files and regenerate types
for dir in examples/with-*/*/frontend; do
  [ -d "$dir" ] || continue
  [ -f "$dir/.env" ] || echo 'PUBLIC_API_URL=http://localhost:8080' > "$dir/.env"
done

for example_dir in examples/with-*/*/; do
  [ -f "$example_dir/forge.toml" ] || continue
  echo "Regenerating types for $(basename "$example_dir")"
  cd "$example_dir" && "$FORGE" generate -y && cd "$GITHUB_WORKSPACE"
done

echo "Done. Verify with: git diff --stat"

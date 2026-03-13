#!/usr/bin/env bash
set -euo pipefail

if [ ! -f "CHANGELOG.md" ]; then
  echo "::error::CHANGELOG.md not found"
  exit 1
fi

VERSION=$(grep -E '^\#\# \[[0-9]+\.[0-9]+\.[0-9]+' CHANGELOG.md | head -1 | sed -E 's/^\#\# \[([0-9]+\.[0-9]+\.[0-9]+[^]]*)\].*/\1/')

if [ -z "$VERSION" ]; then
  echo "::error::No version found in CHANGELOG.md"
  exit 1
fi

if git rev-parse "v$VERSION" >/dev/null 2>&1; then
  echo "::error::Tag v$VERSION already exists"
  exit 1
fi

VERSION_LINE=$(grep -E "^\#\# \[$VERSION\]" CHANGELOG.md)
if ! echo "$VERSION_LINE" | grep -qE '\- [0-9]{4}-[0-9]{2}-[0-9]{2}$'; then
  echo "::error::Version $VERSION missing release date"
  exit 1
fi

RELEASE_DATE=$(echo "$VERSION_LINE" | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}$')

UNRELEASED_CONTENT=$(awk '/^\#\# \[Unreleased\]/,/^\#\# \[/ {print}' CHANGELOG.md | tail -n +2 | head -n -1 | grep -v '^$' || true)
if [ -n "$UNRELEASED_CONTENT" ]; then
  echo "::warning::Unreleased section has content"
fi

RELEASE_NOTES=$(awk -v ver="$VERSION" '
  BEGIN { found=0; printing=0 }
  /^## \[/ {
    if (printing) exit
    if (index($0, "["ver"]")) { found=1; printing=1; next }
  }
  printing { print }
' CHANGELOG.md)

if [ -z "$RELEASE_NOTES" ]; then
  echo "::error::No release notes for version $VERSION"
  exit 1
fi

IS_PRERELEASE="false"
echo "$VERSION" | grep -qE '(alpha|beta|rc)' && IS_PRERELEASE="true"

echo "version=$VERSION" >> "$GITHUB_OUTPUT"
echo "release_date=$RELEASE_DATE" >> "$GITHUB_OUTPUT"
echo "is_prerelease=$IS_PRERELEASE" >> "$GITHUB_OUTPUT"
{
  echo "release_notes<<RELEASE_NOTES_EOF"
  echo "$RELEASE_NOTES"
  echo "RELEASE_NOTES_EOF"
} >> "$GITHUB_OUTPUT"

echo "Version: $VERSION | Date: $RELEASE_DATE | Prerelease: $IS_PRERELEASE"

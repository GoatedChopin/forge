#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARCHIVE_DIR="${ROOT_DIR}/crates/forge/generated"
ARCHIVE_PATH="${ARCHIVE_DIR}/examples.tar"
STAGING_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "${STAGING_DIR}"
}
trap cleanup EXIT

mkdir -p "${ARCHIVE_DIR}"

copy_template() {
  local src="$1"
  local dest="$2"

  mkdir -p "${dest}"

  rsync -a \
    --exclude='.git/' \
    --exclude='pg_data/' \
    --exclude='target/' \
    --exclude='node_modules/' \
    --exclude='.svelte-kit/' \
    --exclude='build/' \
    --exclude='dist/' \
    --exclude='playwright-report/' \
    --exclude='test-results/' \
    --exclude='.forge-dev-integration.log' \
    --exclude='skills/' \
    --exclude='package-lock.json' \
    --exclude='bun.lock' \
    --exclude='Cargo.lock' \
    --exclude='frontend/Cargo.lock' \
    --exclude='frontend/target/' \
    "${src}/" "${dest}/"
}

for framework_dir in "${ROOT_DIR}"/examples/with-*; do
  [ -d "${framework_dir}" ] || continue
  framework_name="$(basename "${framework_dir}")"

  for template_dir in "${framework_dir}"/*; do
    [ -d "${template_dir}" ] || continue
    template_name="$(basename "${template_dir}")"
    copy_template "${template_dir}" "${STAGING_DIR}/${framework_name}/${template_name}"
  done
done

tar -cf "${ARCHIVE_PATH}" -C "${STAGING_DIR}" .
echo "Wrote ${ARCHIVE_PATH}"

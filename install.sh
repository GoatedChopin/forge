#!/bin/sh
set -e

REPO="isala404/forge"
BINARY_NAME="forge"
INSTALL_DIR="${FORGE_INSTALL_DIR:-/usr/local/bin}"

main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    get_architecture || return 1
    local _arch="$RETVAL"

    local _url
    _url="$(get_latest_release_url "$_arch")" || return 1

    local _tmpdir
    _tmpdir="$(mktemp -d)" || return 1
    trap "rm -rf $_tmpdir" EXIT

    echo "Downloading forge from $_url"
    curl -fsSL "$_url" -o "$_tmpdir/forge.tar.gz"

    echo "Extracting..."
    tar -xzf "$_tmpdir/forge.tar.gz" -C "$_tmpdir"

    if [ ! -w "$INSTALL_DIR" ]; then
        echo "Installing to $INSTALL_DIR (requires sudo)"
        sudo mkdir -p "$INSTALL_DIR"
        sudo mv "$_tmpdir/$BINARY_NAME" "$INSTALL_DIR/"
        sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
    else
        mkdir -p "$INSTALL_DIR"
        mv "$_tmpdir/$BINARY_NAME" "$INSTALL_DIR/"
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi

    echo ""
    echo "forge installed to $INSTALL_DIR/$BINARY_NAME"
    echo ""

    if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
        echo "Add $INSTALL_DIR to your PATH:"
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
        echo ""
    fi

    echo "Run 'forge --help' to get started"
}

get_architecture() {
    local _os _arch _target

    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Linux)
            _os="unknown-linux-gnu"
            ;;
        Darwin)
            _os="apple-darwin"
            ;;
        *)
            echo "Unsupported OS: $_os" >&2
            return 1
            ;;
    esac

    case "$_arch" in
        x86_64|amd64)
            _arch="x86_64"
            ;;
        aarch64|arm64)
            _arch="aarch64"
            ;;
        *)
            echo "Unsupported architecture: $_arch" >&2
            return 1
            ;;
    esac

    _target="${_arch}-${_os}"
    RETVAL="$_target"
}

get_latest_release_url() {
    local _arch="$1"
    local _release_url _asset_url

    _release_url="https://api.github.com/repos/$REPO/releases/latest"

    _asset_url=$(curl -fsSL "$_release_url" | grep "browser_download_url" | grep "$_arch" | grep ".tar.gz\"" | grep -v ".sha256" | head -1 | cut -d '"' -f 4)

    if [ -z "$_asset_url" ]; then
        echo "Could not find release for architecture: $_arch" >&2
        return 1
    fi

    echo "$_asset_url"
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        echo "Required command not found: $1" >&2
        return 1
    fi
}

main "$@"

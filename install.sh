#!/bin/sh
# xuanji install script
# Usage: curl -fsSL https://raw.githubusercontent.com/TrueAI-llm/xuanji/master/install.sh | sh
# Or:   curl -fsSL https://raw.githubusercontent.com/TrueAI-llm/xuanji/master/install.sh | sh -s -- --bin-dir /usr/local/bin

set -eu

REPO="TrueAI-llm/xuanji"
GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"

# Defaults
BIN_DIR=""
FORCE=false

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --bin-dir)
            BIN_DIR="$2"
            shift 2
            ;;
        --force)
            FORCE=true
            shift
            ;;
        -h|--help)
            echo "Usage: curl -fsSL .../install.sh | sh -s -- [--bin-dir DIR] [--force]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

# ─── Detect platform ───

OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux)
        TARGET_OS="unknown-linux-gnu"
        ;;
    Darwin)
        TARGET_OS="apple-darwin"
        ;;
    *)
        echo "Unsupported OS: ${OS}"
        exit 1
        ;;
esac

case "${ARCH}" in
    x86_64|amd64)
        TARGET_ARCH="x86_64"
        ;;
    aarch64|arm64)
        TARGET_ARCH="aarch64"
        ;;
    *)
        echo "Unsupported architecture: ${ARCH}"
        exit 1
        ;;
esac

TARGET="${TARGET_ARCH}-${TARGET_OS}"

echo "Platform: ${OS} ${ARCH} -> ${TARGET}"

# ─── Determine install directory ───

if [ -z "${BIN_DIR}" ]; then
    if [ -w "${HOME}/.local/bin" ] || mkdir -p "${HOME}/.local/bin" 2>/dev/null; then
        BIN_DIR="${HOME}/.local/bin"
    elif [ -w "/usr/local/bin" ]; then
        BIN_DIR="/usr/local/bin"
    else
        BIN_DIR="${HOME}/.local/bin"
        mkdir -p "${BIN_DIR}"
    fi
fi

echo "Install directory: ${BIN_DIR}"

# ─── Check if already installed ───

INSTALL_PATH="${BIN_DIR}/xuanji"
if [ -f "${INSTALL_PATH}" ] && [ "${FORCE}" != "true" ]; then
    EXISTING_VERSION=""
    if command -v xuanji >/dev/null 2>&1; then
        EXISTING_VERSION="$(xuanji --version 2>/dev/null || echo "unknown")"
    fi
    echo "xuanji is already installed: ${INSTALL_PATH} (${EXISTING_VERSION})"
    echo "Use --force to overwrite, or run: xuanji --version"
    exit 0
fi

# ─── Fetch latest release ───

echo "Fetching latest release from GitHub..."

if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required but not installed"
    exit 1
fi

RELEASE_JSON="$(curl -fsSL "${GITHUB_API}" 2>/dev/null || echo "")"

if [ -z "${RELEASE_JSON}" ]; then
    echo "Failed to fetch release info from GitHub"
    echo "Check your internet connection and try again"
    exit 1
fi

# Extract tag name (e.g., "v0.1.0")
TAG="$(echo "${RELEASE_JSON}" | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/')"

if [ -z "${TAG}" ]; then
    echo "Could not determine latest version"
    exit 1
fi

echo "Latest version: ${TAG}"

# ─── Download binary ───

FILENAME="xuanji-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${FILENAME}"

echo "Downloading ${FILENAME}..."

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

HTTP_CODE="$(curl -fsSL -w '%{http_code}' -o "${TMP_DIR}/${FILENAME}" "${DOWNLOAD_URL}" 2>/dev/null || echo "000")"

if [ "${HTTP_CODE}" != "200" ]; then
    echo "Download failed (HTTP ${HTTP_CODE})"
    echo "URL: ${DOWNLOAD_URL}"
    echo "The release for ${TARGET} may not be available yet."
    echo "Check https://github.com/${REPO}/releases"
    exit 1
fi

# ─── Extract and install ───

echo "Extracting..."
tar xzf "${TMP_DIR}/${FILENAME}" -C "${TMP_DIR}"

# Find the binary
BINARY="${TMP_DIR}/xuanji"
if [ ! -f "${BINARY}" ]; then
    BINARY="$(find "${TMP_DIR}" -name 'xuanji' -type f 2>/dev/null | head -1)"
fi

if [ -z "${BINARY}" ] || [ ! -f "${BINARY}" ]; then
    echo "Could not find xuanji binary in archive"
    exit 1
fi

# Install
mkdir -p "${BIN_DIR}"
cp "${BINARY}" "${INSTALL_PATH}"
chmod +x "${INSTALL_PATH}"

# ─── Verify ───

INSTALLED_VERSION="${TAG}"
if command -v "${INSTALL_PATH}" >/dev/null 2>&1; then
    INSTALLED_VERSION="$("${INSTALL_PATH}" --version 2>/dev/null || echo "${TAG}")"
fi

echo ""
echo "xuanji ${INSTALLED_VERSION} installed to ${INSTALL_PATH}"
echo ""

# Check PATH
case ":${PATH}:" in
    *":${BIN_DIR}:"*)
        ;;
    *)
        echo "${BIN_DIR} is not in your PATH"
        echo "Add it to your shell profile:"
        echo ""
        echo "  echo 'export PATH=\"${BIN_DIR}:\$PATH\"' >> ~/.bashrc"
        echo "  source ~/.bashrc"
        echo ""
        ;;
esac

echo "Next steps:"
echo "  xuanji init              # Interactive setup"
echo "  xuanji \"your task\"       # Run agent"
echo "  xuanji chat              # Interactive chat"
echo "  xuanji daemon install    # Auto-start on boot"

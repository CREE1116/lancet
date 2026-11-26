#!/bin/sh
# Lancet installer for macOS and Linux (downloads verified prebuilt binary; no Rust needed).
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/CREE1116/lancet/main/install.sh | sh
#   LANCET_VERSION=v0.1.0 LANCET_INSTALL_DIR=/usr/local/bin sh install.sh
set -eu

REPO="CREE1116/lancet"
INSTALL_DIR="${LANCET_INSTALL_DIR:-$HOME/.local/bin}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: '$1' is required to install lancet" >&2
    exit 1
  fi
}

need curl
need tar

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      x86_64)        TARGET="x86_64-apple-darwin" ;;
      *) echo "error: unsupported macOS architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64|amd64)  CPU="x86_64" ;;
      aarch64|arm64) CPU="aarch64" ;;
      *) echo "error: unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
    esac
    # The gnu build links glibc 2.35 (Ubuntu 22.04). Older glibc, musl (Alpine) or
    # unknown libc gets the static musl build. Override with LANCET_LIBC=gnu|musl.
    LIBC="${LANCET_LIBC:-}"
    if [ -z "$LIBC" ]; then
      GLIBC="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')"
      LIBC=musl
      if [ -n "$GLIBC" ]; then
        MAJ="${GLIBC%%.*}"; MIN="${GLIBC#*.}"; MIN="${MIN%%.*}"
        if [ "$MAJ" -gt 2 ] || { [ "$MAJ" -eq 2 ] && [ "$MIN" -ge 35 ]; }; then LIBC=gnu; fi
      fi
    fi
    TARGET="${CPU}-unknown-linux-${LIBC}"
    ;;
  *)
    echo "error: unsupported OS '$OS' (use install.ps1 on Windows)" >&2
    exit 1
    ;;
esac

if [ -n "${LANCET_VERSION:-}" ]; then
  TAG="$LANCET_VERSION"
else
  # Follow github.com's /releases/latest redirect: unlike api.github.com it has no
  # 60-requests-per-hour limit per IP (shared CI runners and NAT hit that).
  TAG="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest" \
    | sed -n 's#.*/releases/tag/\([^/?]*\).*#\1#p')"
  if [ -z "$TAG" ]; then
    echo "error: could not determine latest release tag from GitHub" >&2
    exit 1
  fi
fi

ARCHIVE="lancet-${TAG}-${TARGET}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading ${ARCHIVE} (${TAG})..."
curl -fsSL "${BASE_URL}/${ARCHIVE}" -o "${TMP}/${ARCHIVE}"
curl -fsSL "${BASE_URL}/SHA256SUMS.txt" -o "${TMP}/SHA256SUMS.txt"

EXPECTED="$(awk -v f="$ARCHIVE" '$2 == f {print $1}' "${TMP}/SHA256SUMS.txt")"
if [ -z "$EXPECTED" ]; then
  echo "error: checksum entry for ${ARCHIVE} not found in SHA256SUMS.txt" >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "${TMP}/${ARCHIVE}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "${TMP}/${ARCHIVE}" | awk '{print $1}')"
else
  echo "error: sha256sum or shasum is required to verify download" >&2
  exit 1
fi

if [ "$EXPECTED" != "$ACTUAL" ]; then
  echo "error: SHA256 mismatch for ${ARCHIVE} (expected $EXPECTED, got $ACTUAL)" >&2
  exit 1
fi

tar -xzf "${TMP}/${ARCHIVE}" -C "${TMP}"
mkdir -p "$INSTALL_DIR"
install -m 0755 "${TMP}/lancet" "${INSTALL_DIR}/lancet"

echo "Installed lancet ${TAG} to ${INSTALL_DIR}/lancet"
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *) echo "Note: add ${INSTALL_DIR} to your PATH (e.g. export PATH=\"${INSTALL_DIR}:\$PATH\")" ;;
esac

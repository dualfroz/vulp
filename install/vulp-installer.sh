#!/usr/bin/env bash
# Installs the latest vulp release for Linux x86_64.
# Usage: curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/install/vulp-installer.sh | bash
set -euo pipefail

REPO="${VULP_REPO:-dualfroz/vulp}"
INSTALL_DIR="${VULP_INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="vulp"

os="$(uname -s)"
arch="$(uname -m)"
if [ "$os" != "Linux" ] || [ "$arch" != "x86_64" ]; then
  echo "vulp-installer: no prebuilt binary for $os/$arch (only Linux x86_64 is published)." >&2
  exit 1
fi

echo "Fetching latest vulp release info from github.com/$REPO..."
release_json="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")"
asset_url="$(printf '%s' "$release_json" \
  | grep -o '"browser_download_url": *"[^"]*vulp-linux-x86_64[^"]*"' \
  | head -n1 \
  | sed -E 's/.*"(https:[^"]+)"/\1/')"

if [ -z "$asset_url" ]; then
  echo "vulp-installer: couldn't find a linux-x86_64 asset in the latest release." >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
tmp="$(mktemp)"
echo "Downloading $asset_url..."
curl -fsSL "$asset_url" -o "$tmp"
chmod +x "$tmp"
mv "$tmp" "$INSTALL_DIR/$BIN_NAME"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    shell_rc=""
    case "${SHELL:-}" in
      */zsh)  shell_rc="$HOME/.zshrc" ;;
      */bash) shell_rc="$HOME/.bashrc" ;;
    esac
    if [ -n "$shell_rc" ]; then
      echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$shell_rc"
      echo "Added $INSTALL_DIR to PATH in $shell_rc (restart your shell, or run: export PATH=\"$INSTALL_DIR:\$PATH\")"
    else
      echo "Add $INSTALL_DIR to your PATH to use 'vulp' from anywhere."
    fi
    ;;
esac

echo "Installed vulp to $INSTALL_DIR/$BIN_NAME"
exec "$INSTALL_DIR/$BIN_NAME"

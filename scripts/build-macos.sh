#!/usr/bin/env bash
# Compile Cryptonaute pour macOS et produit un installeur .pkg dans dist/.
# Prérequis : Xcode Command Line Tools, Rust, et Node.js ou la CLI Tauri (cargo install tauri-cli).
# Le .pkg installe Cryptonaute.app dans /Applications et le démon dans
# /Library/PrivilegedHelperTools, puis l'enregistre comme LaunchDaemon.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=$(uname -m)

# CLI Tauri : `cargo tauri` si installée (cargo install tauri-cli), sinon via npx.
if cargo tauri --version >/dev/null 2>&1; then TAURI=(cargo tauri); else TAURI=(npx --yes @tauri-apps/cli@2); fi
(cd crates/app && "${TAURI[@]}" build)

ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
mkdir -p "$ROOT/Applications" "$ROOT/Library/PrivilegedHelperTools"
cp -R target/release/bundle/macos/Cryptonaute.app "$ROOT/Applications/"
install -m 0755 target/release/cryptonaute-service "$ROOT/Library/PrivilegedHelperTools/cryptonaute-service"

mkdir -p dist
chmod +x packaging/macos/scripts/*   # les bits d'exécution peuvent être perdus (dépôt cloné sous Windows)
pkgbuild \
    --root "$ROOT" \
    --scripts packaging/macos/scripts \
    --identifier fr.cryptonaute.pkg \
    --version "$VERSION" \
    --install-location / \
    --ownership recommended \
    "dist/Cryptonaute_${VERSION}_${ARCH}.pkg"
cp packaging/macos/uninstall.sh dist/
echo "Installeur prêt : dist/Cryptonaute_${VERSION}_${ARCH}.pkg"

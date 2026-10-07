#!/usr/bin/env bash
# Compile Cryptonaute pour Linux et produit les paquets .deb et .rpm dans dist/.
# Prérequis (Debian/Ubuntu) :
#   sudo apt install build-essential libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
#        librsvg2-dev libxdo-dev libssl-dev file patchelf rpm nftables
# ainsi que Rust, et Node.js ou la CLI Tauri (cargo install tauri-cli).
set -euo pipefail
cd "$(dirname "$0")/.."
chmod +x packaging/linux/*.sh   # les bits d'exécution peuvent être perdus (dépôt cloné sous Windows)

# CLI Tauri : `cargo tauri` si installée (cargo install tauri-cli), sinon via npx.
if cargo tauri --version >/dev/null 2>&1; then TAURI=(cargo tauri); else TAURI=(npx --yes @tauri-apps/cli@2); fi
(cd crates/app && "${TAURI[@]}" build)

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
mkdir -p dist
# Seuls les paquets de la version en cours (le dossier de build peut en contenir d'anciens).
cp target/release/bundle/deb/*_"${VERSION}"_*.deb target/release/bundle/rpm/*-"${VERSION}"-*.rpm dist/
ls -1 dist/*"${VERSION}"*

#!/usr/bin/env bash
# Compile Cryptonaute pour macOS et produit un installeur .pkg dans dist/.
# Prérequis : Xcode Command Line Tools et Rust.
# Le .pkg installe Cryptonaute.app dans /Applications et le démon dans
# /Library/PrivilegedHelperTools, puis l'enregistre comme LaunchDaemon.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=$(uname -m)
cargo build --release -p cryptonaute -p cryptonaute-service

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

# ---------- Cryptonaute.app ----------
APP="$STAGE/root/Applications/Cryptonaute.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 target/release/cryptonaute "$APP/Contents/MacOS/cryptonaute"

ICONSET="$STAGE/icon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" crates/app/icons/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    if [ "$double" -le 512 ]; then
        sips -z "$double" "$double" crates/app/icons/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
    fi
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>fr</string>
    <key>CFBundleDisplayName</key><string>Cryptonaute</string>
    <key>CFBundleExecutable</key><string>cryptonaute</string>
    <key>CFBundleIconFile</key><string>icon</string>
    <key>CFBundleIdentifier</key><string>fr.cryptonaute.client</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>Cryptonaute</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Cryptonaute contributors — licence MIT</string>
</dict>
</plist>
EOF
# Signature ad hoc : bundle cohérent (sans notarisation, Gatekeeper avertit toujours).
codesign --force --deep --sign - "$APP"

# ---------- Démon et paquet ----------
mkdir -p "$STAGE/root/Library/PrivilegedHelperTools"
install -m 0755 target/release/cryptonaute-service "$STAGE/root/Library/PrivilegedHelperTools/cryptonaute-service"

mkdir -p dist
chmod +x packaging/macos/scripts/*   # les bits d'exécution peuvent être perdus (dépôt cloné sous Windows)
pkgbuild \
    --root "$STAGE/root" \
    --scripts packaging/macos/scripts \
    --identifier fr.cryptonaute.pkg \
    --version "$VERSION" \
    --install-location / \
    --ownership recommended \
    "dist/Cryptonaute_${VERSION}_${ARCH}.pkg"
cp packaging/macos/uninstall.sh dist/
echo "Installeur prêt : dist/Cryptonaute_${VERSION}_${ARCH}.pkg"

#!/usr/bin/env bash
# Compile Cryptonaute pour Linux et produit les paquets .deb et .rpm dans dist/.
# Prérequis (Debian/Ubuntu) :
#   sudo apt install build-essential pkg-config dpkg-dev rpm
# ainsi que Rust. L'interface native (egui) n'a besoin ni de WebKitGTK ni de GTK.
set -euo pipefail
cd "$(dirname "$0")/.."
chmod +x packaging/linux/*.sh   # les bits d'exécution peuvent être perdus (dépôt cloné sous Windows)

cargo build --release -p cryptonaute -p cryptonaute-service

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
DESCRIPTION="Client WireGuard pour Windows, Linux et macOS, utilisable sans droits administrateur après l'installation."
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

# ---------- Arborescence installée ----------
ROOT="$STAGE/root"
install -Dm755 target/release/cryptonaute "$ROOT/usr/bin/cryptonaute"
install -Dm755 target/release/cryptonaute-service "$ROOT/usr/bin/cryptonaute-service"
install -Dm644 crates/app/icons/icon.png "$ROOT/usr/share/icons/hicolor/512x512/apps/cryptonaute.png"
install -Dm644 LICENSE "$ROOT/usr/share/doc/cryptonaute/copyright"
mkdir -p "$ROOT/usr/share/applications"
cat > "$ROOT/usr/share/applications/cryptonaute.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Cryptonaute
Comment=Client WireGuard
Exec=cryptonaute
Icon=cryptonaute
StartupWMClass=cryptonaute
Categories=Network;Utility;
Terminal=false
EOF

mkdir -p dist
rm -f dist/*_"${VERSION}"_*.deb dist/*-"${VERSION}"-*.rpm

# ---------- .deb ----------
DEB="$STAGE/deb"
cp -a "$ROOT" "$DEB"
mkdir -p "$DEB/DEBIAN"
install -m755 packaging/linux/deb-postinst.sh "$DEB/DEBIAN/postinst"
install -m755 packaging/linux/deb-prerm.sh "$DEB/DEBIAN/prerm"
cat > "$DEB/DEBIAN/control" <<EOF
Package: cryptonaute
Version: ${VERSION}
Architecture: amd64
Maintainer: Cryptonaute contributors
Installed-Size: $(du -sk "$ROOT" | cut -f1)
Depends: libc6 (>= 2.31), nftables, libxkbcommon0, libegl1 | libvulkan1
Recommends: xdg-desktop-portal, fonts-dejavu-core | fonts-noto-core
Conflicts: rust-guard
Replaces: rust-guard
Section: net
Priority: optional
Homepage: https://github.com/W3liseth/Cryptonaute
Description: Client WireGuard
 ${DESCRIPTION}
EOF
dpkg-deb --root-owner-group --build "$DEB" "dist/Cryptonaute_${VERSION}_amd64.deb" >/dev/null

# ---------- .rpm ----------
TOP="$STAGE/rpmbuild"
mkdir -p "$TOP"/{BUILD,RPMS,SPECS}
FILES=$(cd "$ROOT" && find . -type f | sed 's|^\.||')
cat > "$TOP/SPECS/cryptonaute.spec" <<EOF
Name: Cryptonaute
Version: ${VERSION}
Release: 1
Summary: Client WireGuard
License: MIT
URL: https://github.com/W3liseth/Cryptonaute
Requires: nftables
Conflicts: RustGuard
Obsoletes: RustGuard
AutoReqProv: no
%global debug_package %{nil}
%global __os_install_post %{nil}

%description
${DESCRIPTION}

%install
mkdir -p %{buildroot}
cp -a ${ROOT}/. %{buildroot}/

%post
$(cat packaging/linux/rpm-post.sh)

%preun
$(cat packaging/linux/rpm-preun.sh)

%files
${FILES}
EOF
rpmbuild -bb --quiet --target x86_64 --define "_topdir $TOP" "$TOP/SPECS/cryptonaute.spec"
cp "$TOP"/RPMS/x86_64/Cryptonaute-"${VERSION}"-1.x86_64.rpm dist/

ls -1 dist/*"${VERSION}"*

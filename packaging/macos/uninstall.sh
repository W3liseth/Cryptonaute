#!/bin/sh
# Désinstalle Cryptonaute de macOS. Usage : sudo sh uninstall.sh
# Les configurations des utilisateurs (~/Library/Application Support/Cryptonaute) sont conservées.
set -e
if [ "$(id -u)" -ne 0 ]; then
    echo "Lancez ce script avec sudo." >&2
    exit 1
fi
HELPER=/Library/PrivilegedHelperTools/cryptonaute-service
[ -x "$HELPER" ] && "$HELPER" uninstall || true
pkill -x cryptonaute 2>/dev/null || true
rm -rf /Applications/Cryptonaute.app "$HELPER" /var/log/cryptonaute.log
pkgutil --forget fr.cryptonaute.pkg >/dev/null 2>&1 || true
echo "Cryptonaute a été désinstallé."

#!/bin/sh
# Paquet .deb : arrête et supprime le démon, sauf lors d'une mise à jour.
set -e
case "$1" in
    remove|deconfigure)
        /usr/bin/cryptonaute-service uninstall || true
        ;;
esac
exit 0

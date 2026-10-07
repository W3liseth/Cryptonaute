#!/bin/sh
# Paquet .deb : installe (ou met à jour) et démarre le démon Cryptonaute.
set -e
case "$1" in
    configure)
        /usr/bin/cryptonaute-service install \
            || echo "Cryptonaute : le démon n'a pas pu être démarré (systemd est-il disponible ?)" >&2
        ;;
esac
exit 0

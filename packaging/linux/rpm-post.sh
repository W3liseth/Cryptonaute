# Paquet .rpm : installe (ou met à jour) et démarre le démon Cryptonaute.
/usr/bin/cryptonaute-service install \
    || echo "Cryptonaute : le démon n'a pas pu être démarré (systemd est-il disponible ?)" >&2
exit 0

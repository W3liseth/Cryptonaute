# Paquet .rpm : $1 vaut 0 lors d'une désinstallation complète, 1 lors d'une mise à jour
# (le %preun de l'ancienne version s'exécute alors après le %post de la nouvelle).
if [ "$1" = "0" ]; then
    /usr/bin/cryptonaute-service uninstall || true
fi
exit 0

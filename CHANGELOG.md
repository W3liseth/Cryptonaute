# Patch notes

Toutes les évolutions notables de Cryptonaute, de la plus récente à la plus ancienne.
Chaque section `## X.Y.Z` est aussi utilisée comme notes de la release GitHub et
affichée dans l'application (*Options › Patch notes*).

## 0.5.1 — 2026-10-09

### Nouveautés
- Bouton **Patch notes** dans les options : l'historique des versions, consultable hors ligne.
- Les notes des releases GitHub (et de la fenêtre de mise à jour) reprennent ce journal, en français.

### Corrections
- **Quitter Cryptonaute déconnecte désormais les tunnels.** Le service reste démarré (il est nécessaire pour se connecter sans droits administrateur), mais plus aucun tunnel ne reste actif une fois l'application quittée.
- Si l'application s'arrête brutalement (plantage, fermeture de session), le service déconnecte les tunnels au bout de 15 secondes, sauf si elle est relancée entre-temps.
- Avec plusieurs sessions ouvertes sur le poste, les tunnels sont déconnectés quand la dernière application est quittée.
- Le menu de l'icône de notification affiche « Quitter et déconnecter » lorsqu'un tunnel est actif.

## 0.5.0 — 2026-10-09

### Nouveautés
- **Mises à jour automatiques** : recherche des nouvelles versions au démarrage puis toutes les 6 heures (désactivable), bandeau dans la barre latérale et entrée dans le menu de l'icône de notification.
- Installation en un clic : l'installeur complet est téléchargé, sa signature vérifiée, puis lancé ; l'application **et** le service sont mis à jour ensemble.
- Bouton « Rechercher une mise à jour » dans les options.

### Sécurité
- Installeurs signés (minisign, Ed25519) par la CI ; l'application refuse tout installeur dont la signature ou le nom ne correspondent pas.

## 0.4.0 — 2026-10-09

### Nouveautés
- **Plusieurs tunnels simultanés** : un tunnel complet (ex. une sortie aux États-Unis) et des tunnels partiels (ex. votre infrastructure) peuvent être actifs ensemble ; les réseaux des tunnels partiels restent prioritaires.
- Le serveur d'un tunnel partiel est joint directement, sans transiter par le tunnel complet.
- Un seul tunnel complet à la fois (en activer un second remplace le premier) ; les tunnels dont les réseaux se chevauchent sont refusés avec un message explicite.
- L'interface indique comment le trafic est réparti entre les tunnels ; débits et graphique par tunnel.
- Menu de l'icône de notification : une case par tunnel et « Tout déconnecter ».

### Corrections
- Linux : les routes vers les serveurs VPN sont retirées à la déconnexion.

## 0.3.0 — 2026-10-07

### Nouveautés
- **RustGuard devient Cryptonaute.** Vos tunnels, options et lancement au démarrage sont repris automatiquement.
- Versions **Linux** (paquets `.deb` et `.rpm`, tunnel complet via nftables) et **macOS** (paquet `.pkg`, Apple Silicon).
- Releases publiées automatiquement sur GitHub avec les installeurs des trois systèmes.

## 0.2.1

### Améliorations
- Bouton **Options** regroupant l'affichage de l'icône de notification et le lancement au démarrage.

## 0.2.0

### Nouveautés
- Option de lancement à l'ouverture de session.

## 0.1.0

### Nouveautés
- Première version : client WireGuard pour Windows, utilisable sans droits administrateur après l'installation.
- Ajout, modification, suppression et import de configurations, chiffrées dans le profil de l'utilisateur.
- Interface animée avec statistiques en temps réel, icône dans la zone de notification.

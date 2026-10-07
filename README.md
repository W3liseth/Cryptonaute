<p align="center">
  <img src="crates/app/icons/icon.png" alt="Cryptonaute" width="112">
</p>

<h1 align="center">Cryptonaute</h1>

<p align="center">
  Client <a href="https://www.wireguard.com/">WireGuard</a> moderne écrit en Rust, pour <b>Windows</b>, <b>Linux</b> et <b>macOS</b>.<br>
  Après une installation unique, <b>aucun droit administrateur</b> n'est nécessaire pour gérer ses tunnels et s'y connecter.
</p>

---

## Sommaire

- [Fonctionnalités](#fonctionnalités)
- [Plateformes prises en charge](#plateformes-prises-en-charge)
- [Installation](#installation)
- [Utilisation](#utilisation)
- [Architecture](#architecture)
- [Sécurité](#sécurité)
- [Compiler depuis les sources](#compiler-depuis-les-sources)
- [Développement](#développement)
- [Structure du dépôt](#structure-du-dépôt)
- [Limites connues et pistes](#limites-connues-et-pistes)
- [Licences](#licences)

## Fonctionnalités

- **Sans droits administrateur au quotidien** : ajouter, modifier, supprimer des
  configurations, se connecter et se déconnecter se font avec un compte standard.
- **Import simple** : glisser-déposer d'un ou plusieurs fichiers `.conf`, sélection de
  fichier, ou création d'un tunnel avec génération d'une paire de clés.
- **Éditeur intégré** avec coloration syntaxique et validation en direct (adresses,
  clés, endpoint, avertissements sur les directives ignorées).
- **Interface moderne et animée** : thème sombre « verre dépoli » dont les couleurs
  suivent l'état de connexion, bouton de connexion animé, graphique du trafic en
  temps réel, statistiques (octets reçus/envoyés, débits, dernière poignée de main,
  durée de connexion).
- **Icône dans la zone de notification** (ou la barre des menus sous macOS) : verte
  lorsqu'un tunnel est actif, menu pour (dé)connecter un tunnel en un clic.
- **Options** : affichage de l'icône de notification, lancement à l'ouverture de session.
- **Tunnel complet ou partiel**, DNS du tunnel, MTU, clé pré-partagée, keepalive.
- **Configurations protégées** dans le profil de l'utilisateur (voir [Sécurité](#sécurité)).

## Plateformes prises en charge

| | Windows 10 (2004+) / 11 | Linux | macOS 11+ |
|---|---|---|---|
| Moteur WireGuard | [WireGuardNT](https://git.zx2c4.com/wireguard-nt/about/) (pilote noyau officiel) | module noyau WireGuard, repli sur [boringtun](https://github.com/cloudflare/boringtun) | boringtun (espace utilisateur) |
| Service privilégié | service Windows (SYSTEM) | démon systemd (root) | LaunchDaemon (root) |
| Canal IPC | named pipe | socket Unix | socket Unix |
| Stockage des configurations | chiffrées avec DPAPI | fichiers privés (0600) | fichiers privés (0600) |
| Lancement au démarrage | registre `HKCU\…\Run` | `~/.config/autostart` (XDG) | `~/Library/LaunchAgents` |
| Installeur | `.exe` (NSIS) | `.deb`, `.rpm` | `.pkg` |

Sous Linux, le DNS du tunnel est configuré via **systemd-resolved** ou **resolvconf**,
et le tunnel complet utilise **nftables** (dépendance des paquets), comme `wg-quick`.

## Installation

L'installation est la **seule** étape qui demande des droits administrateur : elle
installe et démarre le service privilégié.

Les installeurs de chaque version sont disponibles sur la page
[Releases](https://github.com/W3liseth/Cryptonaute/releases/latest).

### Windows

Lancez `Cryptonaute_<version>_x64-setup.exe` et acceptez l'invite UAC. L'application est
installée pour tous les utilisateurs dans `C:\Program Files\Cryptonaute`, avec le service
« Cryptonaute VPN ». La désinstallation se fait depuis *Paramètres › Applications installées*.

### Linux (Debian, Ubuntu, Fedora, openSUSE…)

```bash
sudo apt install ./Cryptonaute_<version>_amd64.deb      # Debian, Ubuntu
sudo dnf install ./Cryptonaute-<version>-1.x86_64.rpm   # Fedora, RHEL
```

Le paquet installe l'application (`/usr/bin/cryptonaute`), le démon
(`/usr/bin/cryptonaute-service`) et active le service systemd `cryptonaute`.
Désinstallation : `sudo apt remove cryptonaute` ou `sudo dnf remove Cryptonaute`.

### macOS

Ouvrez `Cryptonaute_<version>_arm64.pkg`. Le paquet installe `Cryptonaute.app` dans
`/Applications` et le démon dans `/Library/PrivilegedHelperTools`, enregistré comme
LaunchDaemon. Désinstallation : `sudo sh Cryptonaute_<version>_macos-uninstall.sh`
(fourni avec le paquet dans la release, ou `uninstall.sh` après un build local).

> Les paquets ne sont pas signés : Windows SmartScreen et macOS Gatekeeper afficheront
> un avertissement. Voir [Limites connues](#limites-connues-et-pistes).

Dans tous les cas, les configurations des utilisateurs sont conservées à la désinstallation.

### Mise à jour depuis RustGuard

Cryptonaute s'appelait **RustGuard** jusqu'à la version 0.2.1. Installer Cryptonaute
par-dessus suffit, sans désinstaller l'ancienne version au préalable :

- **l'installeur** retire l'ancienne version et son service (installeur Windows,
  `Conflicts`/`Replaces` du `.deb`, `Obsoletes` du `.rpm`, pré-installation du `.pkg`) ;
- **l'application**, à son premier lancement, reprend les tunnels et les options de
  chaque utilisateur (dossier de données, configurations chiffrées DPAPI rechiffrées au
  nouveau format, lancement à l'ouverture de session).

## Utilisation

| Action | Comment |
|---|---|
| Importer une configuration | bouton *Importer*, `Ctrl+O` (`⌘O`), ou glisser-déposer de fichiers `.conf` |
| Créer un tunnel | *Nouveau* (`Ctrl+N`) : une paire de clés est générée, il reste à compléter le pair |
| Se connecter / se déconnecter | bouton central, `Entrée`, double-clic dans la liste, ou menu de l'icône de notification |
| Modifier / supprimer | boutons en haut à droite du tunnel, `Suppr` pour supprimer |
| Options | bouton ⚙ en bas de la barre latérale, `Ctrl+,` (`⌘,`), ou *Options…* dans le menu de l'icône |

Un seul tunnel est actif à la fois : activer un autre tunnel bascule automatiquement.
Fermer la fenêtre la masque dans la zone de notification (si l'icône est activée) ;
le tunnel reste actif, géré par le service, même lorsque l'application est quittée.

Les directives `PreUp`, `PostUp`, `PreDown`, `PostDown`, `Table`, `FwMark` et `SaveConfig`
sont ignorées (et signalées à l'import) ; voir [Sécurité](#sécurité).

## Architecture

```
┌───────────────────────────────┐        IPC local         ┌──────────────────────────────────┐
│ cryptonaute (application Tauri) │   JSON, une requête      │ cryptonaute-service                │
│ compte utilisateur standard   │   par ligne              │ SYSTEM (Windows) / root (Unix)   │
│                               │ ───────────────────────▶ │                                  │
│ • interface HTML/CSS/JS       │  named pipe (Windows)    │ • revalide la configuration      │
│ • stockage des configurations │  socket Unix (Linux/mac) │ • crée l'interface WireGuard     │
│ • icône de notification       │ ◀─────────────────────── │ • adresses, routes, DNS, MTU     │
│ • lancement au démarrage      │        statut            │ • statistiques du tunnel         │
└───────────────────────────────┘                          └──────────────────────────────────┘
```

Créer une interface réseau et modifier la table de routage exige des privilèges élevés
sur tous les systèmes. Comme le client officiel WireGuard, Cryptonaute sépare donc
l'interface (non privilégiée) d'un service installé une seule fois, qui n'expose que
trois opérations : *connecter* (nom + configuration), *déconnecter* et *statut*.

| Crate | Rôle |
|---|---|
| [`crates/common`](crates/common) | Parseur et validateur `wg-quick`, protocole IPC — partagé et testé |
| [`crates/service`](crates/service) | Service privilégié : `windows/` (SCM, WireGuardNT, IP Helper, named pipe) et `unix/` (systemd/launchd, [`defguard_wireguard_rs`](https://github.com/DefGuard/wireguard-rs), socket Unix, nftables) |
| [`crates/app`](crates/app) | Application [Tauri 2](https://tauri.app/) : stockage, client IPC, zone de notification, options |
| [`ui/`](ui) | Interface en HTML/CSS/JS sans framework ni étape de build |

## Sécurité

- **Le client ne parle qu'au vrai service.** Le named pipe est créé avec SYSTEM comme
  propriétaire, et le client le vérifie. Le socket Unix est placé dans un dossier
  appartenant à root, et le client vérifie que le processus à l'autre bout est root
  (`SO_PEERCRED` sous Linux, `getpeereid` sous macOS). Un processus non privilégié ne
  peut donc pas se faire passer pour le service et intercepter une clé privée.
- **Accès au service** : utilisateurs connectés localement uniquement (`IU` sous
  Windows, socket local sous Unix), connexions distantes refusées, taille des messages
  bornée, délai d'inactivité.
- **Aucune exécution de script.** Le service s'exécute avec les plus hauts privilèges :
  exécuter les commandes `PostUp`/`PreUp` fournies par un utilisateur permettrait à
  n'importe qui de prendre le contrôle de la machine. Elles sont donc ignorées.
- **Revalidation côté service** de chaque configuration reçue ; les copies des clés
  secrètes sont effacées de la mémoire dès leur transmission au pilote (Windows).
- **Configurations protégées** : chiffrées avec DPAPI sous Windows (seul le compte
  propriétaire peut les déchiffrer) ; fichiers `0600` dans un dossier `0700` sous
  Linux et macOS, comme `/etc/wireguard` pour `wg-quick`.
- **Binaire du service de confiance** : sous Linux et macOS, `cryptonaute-service install`
  refuse de s'enregistrer si son exécutable (ou un dossier parent) est modifiable par
  un autre utilisateur que root.
- **`wireguard.dll`** est le pilote officiel signé par WireGuard LLC ; le script de mise
  à jour vérifie sa signature Authenticode.

## Compiler depuis les sources

Prérequis communs : [Rust](https://rustup.rs/) stable, et [Node.js](https://nodejs.org/)
(la CLI Tauri est alors récupérée via `npx`) ou la CLI Tauri (`cargo install tauri-cli`).

### Windows

Prérequis : toolchain MSVC (Visual Studio Build Tools).

```powershell
pwsh scripts/build.ps1          # → dist\Cryptonaute_<version>_x64-setup.exe
```

`third_party/wireguard-nt/wireguard.dll` (v0.10.1, amd64) est fourni avec sa licence.
Pour changer de version ou d'architecture : `pwsh scripts/fetch-wireguard-nt.ps1 -Version x.y.z`.

### Linux

```bash
sudo apt install build-essential libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
     librsvg2-dev libxdo-dev libssl-dev file patchelf rpm
bash scripts/build-linux.sh     # → dist/*.deb et dist/*.rpm
```

### macOS

Prérequis : Xcode Command Line Tools (`xcode-select --install`).

```bash
bash scripts/build-macos.sh     # → dist/Cryptonaute_<version>_<arch>.pkg
```

Le paquet est produit pour l'architecture de la machine (`arm64` ou `x86_64`).

### Intégration continue

Le workflow [`.github/workflows/build.yml`](.github/workflows/build.yml) exécute clippy
et les tests sur les trois plateformes, puis produit l'installeur Windows, les paquets
Linux et le paquet macOS (Apple Silicon).

Sur `main`, si la release correspondant à la version de `Cargo.toml` n'existe pas encore,
le workflow crée le tag `vX.Y.Z` et publie la release avec les installeurs en
téléchargement direct (sans archive zip). Pour publier une nouvelle version, il suffit
donc d'incrémenter `version` et de pousser sur `main` ; tant que la version ne change
pas, les builds suivants ne republient rien.

### Version

La version est définie à un seul endroit, dans [`Cargo.toml`](Cargo.toml)
(`[workspace.package]`) ; l'application, le service et les installeurs la reprennent.

## Développement

```bash
cargo test --workspace          # parseur, stockage (chiffrement / permissions)
cargo clippy --workspace --all-targets
node scripts/serve-ui.mjs       # interface en mode démo dans un navigateur : http://localhost:5173
```

Le **mode démo** de l'interface (activé automatiquement hors de Tauri) simule le service
et le trafic : pratique pour travailler le design sans compiler l'application.

Exécuter le service au premier plan, avec les journaux dans la console :

| Plateforme | Commande | Journaux du service installé |
|---|---|---|
| Windows | `cryptonaute-service console` (administrateur) | `C:\Program Files\Cryptonaute\logs\service.log` |
| Linux | `sudo cryptonaute-service run` | `journalctl -u cryptonaute` |
| macOS | `sudo cryptonaute-service run` | `/var/log/cryptonaute.log` |

`pwsh scripts/gen-icons.ps1` régénère les icônes (application et zone de notification).

## Structure du dépôt

```
cryptonaute/
├── crates/
│   ├── common/          parseur wg-quick, protocole IPC
│   ├── service/         service privilégié (windows/ et unix/)
│   └── app/             application Tauri (+ tauri.<plateforme>.conf.json, hooks NSIS, icônes)
├── ui/                  interface (index.html, styles.css, app.js)
├── packaging/
│   ├── linux/           scripts des paquets .deb/.rpm
│   └── macos/           scripts du .pkg et désinstallation
├── scripts/             build.ps1, build-linux.sh, build-macos.sh, icônes, serveur de démo
├── third_party/         wireguard.dll (WireGuardNT) et sa licence
└── .github/workflows/   intégration continue
```

## Limites connues et pistes

- **Pas de kill-switch** ni de blocage des fuites DNS hors tunnel (pare-feu WFP sous
  Windows, nftables/pf ailleurs).
- **Un seul tunnel actif** à la fois ; MTU fixe (1420 par défaut) sans détection automatique.
- **Poste partagé** : tout utilisateur connecté peut désactiver le tunnel d'un autre.
- **Paquets non signés** : signature Authenticode (Windows) et signature + notarisation
  Apple (macOS) à mettre en place pour une diffusion publique.
- **macOS** : compilé et vérifié par l'intégration continue, mais pas encore testé sur
  une machine réelle ; seul Apple Silicon est produit par la CI.
- **Linux** : le DNS du tunnel nécessite systemd-resolved ou resolvconf (la connexion
  échoue avec un message explicite sinon).

## Licences

- Cryptonaute : MIT.
- [WireGuardNT](https://git.zx2c4.com/wireguard-nt/about/) (`wireguard.dll`) : licence
  « Prebuilt Binaries » de WireGuard LLC (voir `third_party/wireguard-nt/LICENSE.txt`),
  qui en autorise la redistribution avec un logiciel l'utilisant via son API.
- [defguard_wireguard_rs](https://github.com/DefGuard/wireguard-rs) : Apache-2.0.

WireGuard est une marque déposée de Jason A. Donenfeld.

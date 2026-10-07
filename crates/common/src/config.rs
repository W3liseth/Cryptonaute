//! Analyse et validation des fichiers de configuration au format `wg-quick`.
//!
//! Le même parseur est utilisé par l'interface (pour l'aperçu et la validation
//! à l'import) et par le service, qui ne fait jamais confiance au client et
//! revalide systématiquement la configuration reçue.
//!
//! Les directives `PreUp`/`PostUp`/`PreDown`/`PostDown` sont volontairement
//! ignorées : le service s'exécute en tant que SYSTEM et exécuter des commandes
//! fournies par un utilisateur non privilégié serait une élévation de privilèges.

use std::fmt;
use std::net::IpAddr;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

/// Longueur maximale d'un nom de tunnel (contrainte par le nom d'adaptateur Windows
/// et par le nom de fichier de stockage).
pub const MAX_TUNNEL_NAME_LEN: usize = 32;

/// Taille maximale acceptée pour un fichier de configuration.
pub const MAX_CONFIG_LEN: usize = 32 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("ligne {line} : {msg}")]
    Line { line: usize, msg: String },
    #[error("{0}")]
    Invalid(String),
}

fn line_err(line: usize, msg: impl Into<String>) -> ConfigError {
    ConfigError::Line {
        line,
        msg: msg.into(),
    }
}

/// Clé Curve25519 (privée, publique ou pré-partagée). Effacée de la mémoire à la libération.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(pub [u8; 32]);

impl Key {
    pub fn parse(s: &str) -> Result<Self, String> {
        let bytes = B64
            .decode(s.trim())
            .map_err(|_| "clé base64 invalide".to_string())?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "une clé doit faire exactement 32 octets".to_string())?;
        Ok(Key(arr))
    }

    pub fn generate() -> Self {
        Key(StaticSecret::random().to_bytes())
    }

    pub fn to_base64(&self) -> String {
        B64.encode(self.0)
    }

    /// Calcule la clé publique correspondant à cette clé privée.
    pub fn public_key(&self) -> Key {
        let secret = StaticSecret::from(self.0);
        Key(PublicKey::from(&secret).to_bytes())
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(…)")
    }
}

/// Point de terminaison d'un pair : nom d'hôte ou adresse IP, et port UDP.
/// La résolution DNS est faite par le service au moment de la connexion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (host, port) = if let Some(rest) = s.strip_prefix('[') {
            let (host, port) = rest
                .split_once("]:")
                .ok_or("endpoint IPv6 attendu sous la forme [adresse]:port")?;
            (host, port)
        } else {
            let (host, port) = s
                .rsplit_once(':')
                .ok_or("endpoint attendu sous la forme hôte:port")?;
            if host.contains(':') {
                return Err("une adresse IPv6 doit être entourée de crochets : [adresse]:port".into());
            }
            (host, port)
        };
        if host.is_empty() {
            return Err("hôte de l'endpoint manquant".into());
        }
        if !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_' | '%'))
        {
            return Err("nom d'hôte de l'endpoint invalide".into());
        }
        let port: u16 = port.parse().map_err(|_| "port de l'endpoint invalide")?;
        if port == 0 {
            return Err("le port de l'endpoint ne peut pas être 0".into());
        }
        Ok(Endpoint {
            host: host.to_string(),
            port,
        })
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterfaceConfig {
    pub private_key: Key,
    /// Adresses de l'interface, avec leur longueur de préfixe.
    pub addresses: Vec<IpNet>,
    pub dns_servers: Vec<IpAddr>,
    pub dns_search: Vec<String>,
    pub listen_port: Option<u16>,
    pub mtu: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct PeerConfig {
    pub public_key: Key,
    pub preshared_key: Option<Key>,
    /// Réseaux autorisés, normalisés (bits d'hôte à zéro).
    pub allowed_ips: Vec<IpNet>,
    pub endpoint: Option<Endpoint>,
    pub persistent_keepalive: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct WgConfig {
    pub interface: InterfaceConfig,
    pub peers: Vec<PeerConfig>,
    /// Avertissements non bloquants (directives ignorées, etc.).
    pub warnings: Vec<String>,
}

#[derive(Default)]
struct PartialInterface {
    private_key: Option<Key>,
    addresses: Vec<IpNet>,
    dns_servers: Vec<IpAddr>,
    dns_search: Vec<String>,
    listen_port: Option<u16>,
    mtu: Option<u16>,
}

#[derive(Default)]
struct PartialPeer {
    start_line: usize,
    public_key: Option<Key>,
    preshared_key: Option<Key>,
    allowed_ips: Vec<IpNet>,
    endpoint: Option<Endpoint>,
    persistent_keepalive: Option<u16>,
}

enum Section {
    None,
    Interface,
    Peer,
}

fn split_list(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|s| !s.is_empty())
}

fn parse_net(s: &str, normalize: bool) -> Result<IpNet, String> {
    let net = if s.contains('/') {
        s.parse::<IpNet>()
            .map_err(|_| format!("« {s} » n'est pas un réseau valide"))?
    } else {
        let ip: IpAddr = s
            .parse()
            .map_err(|_| format!("« {s} » n'est pas une adresse IP valide"))?;
        IpNet::from(ip)
    };
    Ok(if normalize { net.trunc() } else { net })
}

impl WgConfig {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        if text.len() > MAX_CONFIG_LEN {
            return Err(ConfigError::Invalid(
                "configuration trop volumineuse (32 Kio maximum)".into(),
            ));
        }

        let mut section = Section::None;
        let mut interface: Option<PartialInterface> = None;
        let mut peers: Vec<PartialPeer> = Vec::new();
        let mut warnings = Vec::new();

        for (idx, raw) in text.lines().enumerate() {
            let line_no = idx + 1;
            let line = raw
                .split(['#', ';'])
                .next()
                .unwrap_or_default()
                .trim();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') {
                if !line.ends_with(']') {
                    return Err(line_err(line_no, "en-tête de section mal formé"));
                }
                match line[1..line.len() - 1].trim().to_ascii_lowercase().as_str() {
                    "interface" => {
                        if interface.is_some() {
                            return Err(line_err(line_no, "une seule section [Interface] est autorisée"));
                        }
                        interface = Some(PartialInterface::default());
                        section = Section::Interface;
                    }
                    "peer" => {
                        peers.push(PartialPeer {
                            start_line: line_no,
                            ..Default::default()
                        });
                        section = Section::Peer;
                    }
                    other => {
                        return Err(line_err(line_no, format!("section inconnue [{other}]")));
                    }
                }
                continue;
            }

            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| line_err(line_no, "ligne attendue sous la forme Clé = Valeur"))?;
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            let err = |msg: String| line_err(line_no, msg);

            match section {
                Section::None => {
                    return Err(line_err(line_no, "directive en dehors de toute section"));
                }
                Section::Interface => {
                    let iface = interface.as_mut().expect("section interface ouverte");
                    match key.as_str() {
                        "privatekey" => {
                            iface.private_key = Some(Key::parse(value).map_err(|e| err(format!("PrivateKey : {e}")))?);
                        }
                        "address" => {
                            for item in split_list(value) {
                                iface.addresses.push(parse_net(item, false).map_err(err)?);
                            }
                        }
                        "dns" => {
                            for item in split_list(value) {
                                match item.parse::<IpAddr>() {
                                    Ok(ip) => iface.dns_servers.push(ip),
                                    Err(_) if is_valid_domain(item) => iface.dns_search.push(item.to_string()),
                                    Err(_) => return Err(err(format!("DNS : « {item} » invalide"))),
                                }
                            }
                        }
                        "listenport" => {
                            iface.listen_port = Some(value.parse().map_err(|_| err("ListenPort invalide".into()))?);
                        }
                        "mtu" => {
                            let mtu: u16 = value.parse().map_err(|_| err("MTU invalide".into()))?;
                            if !(576..=65535).contains(&mtu) {
                                return Err(err("MTU hors limites (576 à 65535)".into()));
                            }
                            iface.mtu = Some(mtu);
                        }
                        "preup" | "postup" | "predown" | "postdown" => warnings.push(format!(
                            "ligne {line_no} : directive {} ignorée (l'exécution de scripts n'est pas prise en charge, par sécurité)",
                            raw.split('=').next().unwrap_or_default().trim()
                        )),
                        "table" | "fwmark" | "saveconfig" => warnings.push(format!(
                            "ligne {line_no} : directive {} ignorée (spécifique à Linux)",
                            raw.split('=').next().unwrap_or_default().trim()
                        )),
                        _ => warnings.push(format!("ligne {line_no} : directive inconnue « {key} » ignorée")),
                    }
                }
                Section::Peer => {
                    let peer = peers.last_mut().expect("section peer ouverte");
                    match key.as_str() {
                        "publickey" => {
                            peer.public_key = Some(Key::parse(value).map_err(|e| err(format!("PublicKey : {e}")))?);
                        }
                        "presharedkey" => {
                            peer.preshared_key = Some(Key::parse(value).map_err(|e| err(format!("PresharedKey : {e}")))?);
                        }
                        "allowedips" => {
                            for item in split_list(value) {
                                peer.allowed_ips.push(parse_net(item, true).map_err(err)?);
                            }
                        }
                        "endpoint" => {
                            peer.endpoint = Some(Endpoint::parse(value).map_err(|e| err(format!("Endpoint : {e}")))?);
                        }
                        "persistentkeepalive" => {
                            let ka = if value.eq_ignore_ascii_case("off") {
                                0
                            } else {
                                value.parse().map_err(|_| err("PersistentKeepalive invalide".into()))?
                            };
                            peer.persistent_keepalive = (ka > 0).then_some(ka);
                        }
                        _ => warnings.push(format!("ligne {line_no} : directive inconnue « {key} » ignorée")),
                    }
                }
            }
        }

        let iface = interface.ok_or_else(|| ConfigError::Invalid("section [Interface] manquante".into()))?;
        let private_key = iface
            .private_key
            .ok_or_else(|| ConfigError::Invalid("PrivateKey manquante dans [Interface]".into()))?;
        if iface.addresses.is_empty() {
            return Err(ConfigError::Invalid("au moins une adresse (Address) est requise dans [Interface]".into()));
        }
        if peers.is_empty() {
            return Err(ConfigError::Invalid("au moins une section [Peer] est requise".into()));
        }

        let mut out_peers: Vec<PeerConfig> = Vec::with_capacity(peers.len());
        for p in peers {
            let at = p.start_line;
            let public_key = p
                .public_key
                .ok_or_else(|| line_err(at, "PublicKey manquante dans [Peer]"))?;
            if out_peers.iter().any(|o| o.public_key == public_key) {
                return Err(line_err(at, "deux pairs ont la même clé publique"));
            }
            let endpoint = p
                .endpoint
                .ok_or_else(|| line_err(at, "Endpoint manquant dans [Peer] (requis pour un client)"))?;
            let mut allowed_ips = p.allowed_ips;
            allowed_ips.sort();
            allowed_ips.dedup();
            out_peers.push(PeerConfig {
                public_key,
                preshared_key: p.preshared_key,
                allowed_ips,
                endpoint: Some(endpoint),
                persistent_keepalive: p.persistent_keepalive,
            });
        }

        Ok(WgConfig {
            interface: InterfaceConfig {
                private_key,
                addresses: iface.addresses,
                dns_servers: iface.dns_servers,
                dns_search: iface.dns_search,
                listen_port: iface.listen_port,
                mtu: iface.mtu,
            },
            peers: out_peers,
            warnings,
        })
    }

    /// Vrai si la configuration route tout le trafic IPv4 ou IPv6 dans le tunnel.
    pub fn is_full_tunnel(&self) -> bool {
        self.peers
            .iter()
            .flat_map(|p| &p.allowed_ips)
            .any(|n| n.prefix_len() == 0)
    }

    pub fn summary(&self) -> TunnelSummary {
        TunnelSummary {
            public_key: self.interface.private_key.public_key().to_base64(),
            addresses: self.interface.addresses.iter().map(ToString::to_string).collect(),
            dns: self
                .interface
                .dns_servers
                .iter()
                .map(ToString::to_string)
                .chain(self.interface.dns_search.iter().cloned())
                .collect(),
            listen_port: self.interface.listen_port,
            mtu: self.interface.mtu,
            full_tunnel: self.is_full_tunnel(),
            peers: self
                .peers
                .iter()
                .map(|p| PeerSummary {
                    public_key: p.public_key.to_base64(),
                    endpoint: p.endpoint.as_ref().map(ToString::to_string),
                    allowed_ips: p.allowed_ips.iter().map(ToString::to_string).collect(),
                    persistent_keepalive: p.persistent_keepalive,
                    has_preshared_key: p.preshared_key.is_some(),
                })
                .collect(),
            warnings: self.warnings.clone(),
        }
    }
}

fn is_valid_domain(s: &str) -> bool {
    s.len() <= 253
        && s.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// Résumé affichable (sans la clé privée) d'une configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelSummary {
    pub public_key: String,
    pub addresses: Vec<String>,
    pub dns: Vec<String>,
    pub listen_port: Option<u16>,
    pub mtu: Option<u16>,
    pub full_tunnel: bool,
    pub peers: Vec<PeerSummary>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerSummary {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
    pub has_preshared_key: bool,
}

/// Valide un nom de tunnel : utilisé comme nom de fichier et nom d'adaptateur réseau.
pub fn validate_tunnel_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("le nom du tunnel est requis".into());
    }
    if name.chars().count() > MAX_TUNNEL_NAME_LEN {
        return Err(format!("le nom du tunnel est limité à {MAX_TUNNEL_NAME_LEN} caractères"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '=' | '+' | '.' | '-'))
    {
        return Err("caractères autorisés : lettres, chiffres et _ = + . -".into());
    }
    if name.starts_with('.') || name.ends_with('.') {
        return Err("le nom ne peut pas commencer ni finir par un point".into());
    }
    let upper = name.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or_default();
    let reserved = ["CON", "PRN", "AUX", "NUL"];
    let is_device = reserved.contains(&stem)
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if is_device {
        return Err("ce nom est réservé par Windows".into());
    }
    Ok(())
}

/// Squelette de configuration avec une paire de clés fraîchement générée.
pub fn new_template() -> String {
    let key = Key::generate();
    format!(
        "[Interface]\n\
         # Clé publique : {}\n\
         PrivateKey = {}\n\
         Address = 10.0.0.2/32\n\
         DNS = 1.1.1.1\n\
         \n\
         [Peer]\n\
         PublicKey = \n\
         AllowedIPs = 0.0.0.0/0, ::/0\n\
         Endpoint = vpn.exemple.fr:51820\n\
         PersistentKeepalive = 25\n",
        key.public_key().to_base64(),
        key.to_base64()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# commentaire
[Interface]
PrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=
Address = 10.192.122.3/24, fd00::3/64
DNS = 10.192.122.1, corp.example
PostUp = iptables -A FORWARD -i %i -j ACCEPT

[Peer]
PublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=
Endpoint = [2001:db8::1]:51820
AllowedIPs = 0.0.0.0/0, 192.168.1.17/24
PersistentKeepalive = 25
";

    #[test]
    fn parses_sample() {
        let cfg = WgConfig::parse(SAMPLE).unwrap();
        assert_eq!(cfg.interface.addresses.len(), 2);
        assert_eq!(cfg.interface.addresses[0].to_string(), "10.192.122.3/24");
        assert_eq!(cfg.interface.dns_servers.len(), 1);
        assert_eq!(cfg.interface.dns_search, vec!["corp.example"]);
        let peer = &cfg.peers[0];
        assert_eq!(peer.endpoint.as_ref().unwrap().to_string(), "[2001:db8::1]:51820");
        assert!(peer.allowed_ips.iter().any(|n| n.to_string() == "192.168.1.0/24"));
        assert_eq!(peer.persistent_keepalive, Some(25));
        assert!(cfg.is_full_tunnel());
        assert_eq!(cfg.warnings.len(), 1, "PostUp doit être signalé et ignoré");
    }

    #[test]
    fn derives_public_key() {
        // Vecteur de test : la clé publique de cette clé privée est connue.
        let k = Key::parse("yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=").unwrap();
        assert_eq!(k.public_key().to_base64(), "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=");
    }

    #[test]
    fn rejects_missing_endpoint() {
        let cfg = SAMPLE.replace("Endpoint = [2001:db8::1]:51820\n", "");
        assert!(WgConfig::parse(&cfg).is_err());
    }

    #[test]
    fn rejects_bad_key() {
        let cfg = SAMPLE.replace("yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=", "abc");
        let err = WgConfig::parse(&cfg).unwrap_err().to_string();
        assert!(err.contains("ligne 3"), "{err}");
    }

    #[test]
    fn endpoint_forms() {
        assert_eq!(Endpoint::parse("vpn.example.com:51820").unwrap().host, "vpn.example.com");
        assert_eq!(Endpoint::parse("1.2.3.4:1").unwrap().port, 1);
        assert!(Endpoint::parse("2001:db8::1:51820").is_err());
        assert!(Endpoint::parse("host:0").is_err());
        assert!(Endpoint::parse(":51820").is_err());
    }

    #[test]
    fn tunnel_names() {
        assert!(validate_tunnel_name("wg0").is_ok());
        assert!(validate_tunnel_name("bureau-paris.2").is_ok());
        assert!(validate_tunnel_name("").is_err());
        assert!(validate_tunnel_name("a b").is_err());
        assert!(validate_tunnel_name("CON").is_err());
        assert!(validate_tunnel_name("com1").is_err());
        assert!(validate_tunnel_name("../x").is_err());
        assert!(validate_tunnel_name(&"x".repeat(33)).is_err());
    }

    #[test]
    fn template_parses_once_filled() {
        let t = new_template().replace(
            "PublicKey = \n",
            "PublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\n",
        );
        WgConfig::parse(&t).unwrap();
    }
}

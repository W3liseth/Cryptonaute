//! Protocole entre l'interface (non privilégiée) et le service privilégié.
//!
//! Transport local, une requête JSON par ligne et une réponse JSON par ligne :
//! - Windows : named pipe créé par le service avec SYSTEM comme propriétaire ;
//! - Linux / macOS : socket Unix dans un dossier appartenant à root.
//!
//! Dans les deux cas, le client vérifie l'identité du serveur (SYSTEM ou root)
//! avant d'envoyer la moindre clé privée (protection contre l'usurpation).

use serde::{Deserialize, Serialize};

/// Named pipe du service (Windows).
pub const PIPE_NAME: &str = r"\\.\pipe\Cryptonaute\service";
/// Socket Unix du démon (Linux et macOS). Son dossier appartient à root.
pub const SOCKET_PATH: &str = "/var/run/cryptonaute/cryptonaute.sock";
pub const PROTOCOL_VERSION: u32 = 1;
/// Taille maximale d'un message (une ligne JSON).
pub const MAX_MESSAGE_LEN: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Hello,
    Status,
    /// Active le tunnel `name` avec la configuration `config` (format wg-quick).
    /// Un éventuel tunnel déjà actif est d'abord arrêté.
    Connect { name: String, config: String },
    Disconnect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Hello { protocol: u32, version: String },
    Status(TunnelStatus),
    Error { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelState {
    Disconnected,
    Connected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub state: TunnelState,
    pub tunnel: Option<String>,
    /// Horodatage Unix (secondes) de l'activation du tunnel.
    pub connected_since: Option<u64>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Horodatage Unix (secondes) de la dernière poignée de main réussie.
    pub last_handshake: Option<u64>,
    /// Endpoint résolu du premier pair.
    pub endpoint: Option<String>,
    /// Dernière erreur rencontrée par le service (échec de connexion, etc.).
    pub last_error: Option<String>,
}

impl TunnelStatus {
    pub fn disconnected(last_error: Option<String>) -> Self {
        TunnelStatus {
            state: TunnelState::Disconnected,
            tunnel: None,
            connected_since: None,
            rx_bytes: 0,
            tx_bytes: 0,
            last_handshake: None,
            endpoint: None,
            last_error,
        }
    }
}

/// Sérialise un message en une ligne JSON terminée par `\n`.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    let mut v = serde_json::to_vec(msg).expect("sérialisation JSON infaillible");
    v.push(b'\n');
    v
}

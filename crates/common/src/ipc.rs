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
/// Version 2 : plusieurs tunnels peuvent être actifs simultanément.
pub const PROTOCOL_VERSION: u32 = 2;
/// Taille maximale d'un message (une ligne JSON).
pub const MAX_MESSAGE_LEN: usize = 64 * 1024;
/// Nombre maximal de tunnels actifs en même temps.
pub const MAX_ACTIVE_TUNNELS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Hello,
    Status,
    /// Active le tunnel `name` avec la configuration `config` (format wg-quick),
    /// à côté des tunnels déjà actifs. Un tunnel du même nom est d'abord arrêté,
    /// de même que l'éventuel autre tunnel complet (un seul à la fois).
    Connect { name: String, config: String },
    /// Arrête le tunnel `name`, ou tous les tunnels si `name` est absent.
    Disconnect {
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Hello { protocol: u32, version: String },
    Status(ServiceStatus),
    Error { message: String },
}

/// État de l'ensemble des tunnels gérés par le service.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    /// Tunnels actifs, dans l'ordre de leur activation.
    pub tunnels: Vec<TunnelStatus>,
    /// Dernière erreur rencontrée par le service (échec de connexion, etc.).
    pub last_error: Option<String>,
}

impl ServiceStatus {
    pub fn tunnel(&self, name: &str) -> Option<&TunnelStatus> {
        self.tunnels.iter().find(|t| t.name == name)
    }

    pub fn is_active(&self, name: &str) -> bool {
        self.tunnel(name).is_some()
    }
}

/// État d'un tunnel actif.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub name: String,
    /// Vrai si le tunnel route tout le trafic (les autres tunnels actifs restent
    /// prioritaires pour leurs propres réseaux).
    pub full_tunnel: bool,
    /// Horodatage Unix (secondes) de l'activation du tunnel.
    pub connected_since: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Horodatage Unix (secondes) de la dernière poignée de main réussie.
    pub last_handshake: Option<u64>,
    /// Endpoint résolu du premier pair.
    pub endpoint: Option<String>,
}

/// Sérialise un message en une ligne JSON terminée par `\n`.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    let mut v = serde_json::to_vec(msg).expect("sérialisation JSON infaillible");
    v.push(b'\n');
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_name_is_optional() {
        let all: Request = serde_json::from_str(r#"{"cmd":"disconnect"}"#).unwrap();
        assert!(matches!(all, Request::Disconnect { name: None }));
        let one: Request = serde_json::from_str(r#"{"cmd":"disconnect","name":"usa"}"#).unwrap();
        assert!(matches!(one, Request::Disconnect { name: Some(n) } if n == "usa"));
    }
}

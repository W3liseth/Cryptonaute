//! Accès aux données de l'interface : configurations (stockage chiffré) et
//! service privilégié (IPC). Les appels bloquants sont exécutés hors du thread
//! de l'interface par l'appelant.
//!
//! En développement, `CRYPTONAUTE_DEMO=1` remplace le stockage et le service par
//! des données simulées : l'interface se teste sans toucher aux tunnels réels.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use cryptonaute_common::config::{TunnelSummary, WgConfig};
use cryptonaute_common::ipc::{Request, Response, ServiceStatus, TunnelStatus};

use crate::client::{self, ClientError};
use crate::store::Store;

/// Un tunnel enregistré, avec son résumé ou l'erreur qui empêche de le lire.
#[derive(Clone)]
pub struct TunnelEntry {
    pub name: String,
    pub summary: Option<TunnelSummary>,
    pub error: Option<String>,
}

pub trait Backend: Send + Sync {
    fn list(&self) -> Result<Vec<TunnelEntry>, String>;
    fn load(&self, name: &str) -> Result<String, String>;
    fn save(&self, name: &str, config: &str, previous: Option<&str>) -> Result<(), String>;
    fn delete(&self, name: &str) -> Result<(), String>;
    fn status(&self) -> Result<ServiceStatus, String>;
    fn version(&self) -> Result<String, String>;
    fn connect(&self, name: &str) -> Result<ServiceStatus, String>;
    fn disconnect(&self, name: Option<&str>) -> Result<ServiceStatus, String>;
}

static BACKEND: OnceLock<Arc<dyn Backend>> = OnceLock::new();

/// Backend de l'application (réel, ou démo en développement).
pub fn get() -> Arc<dyn Backend> {
    BACKEND
        .get_or_init(|| {
            if is_demo() {
                Arc::new(Demo::default())
            } else {
                Arc::new(Real)
            }
        })
        .clone()
}

pub fn is_demo() -> bool {
    cfg!(debug_assertions) && std::env::var_os("CRYPTONAUTE_DEMO").is_some()
}

fn entry(name: String, text: Result<String, String>) -> TunnelEntry {
    match text.and_then(|t| WgConfig::parse(&t).map_err(|e| e.to_string())) {
        Ok(cfg) => TunnelEntry {
            name,
            summary: Some(cfg.summary()),
            error: None,
        },
        Err(e) => TunnelEntry {
            name,
            summary: None,
            error: Some(e),
        },
    }
}

pub fn status_from(resp: Result<Response, ClientError>) -> Result<ServiceStatus, String> {
    match resp.map_err(|e| e.to_string())? {
        Response::Status(s) => Ok(s),
        Response::Error { message } => Err(message),
        Response::Hello { .. } | Response::Session => Err("réponse inattendue du service".into()),
    }
}

/// Stockage chiffré de l'utilisateur et service installé.
struct Real;

impl Backend for Real {
    fn list(&self) -> Result<Vec<TunnelEntry>, String> {
        let store = Store::open()?;
        Ok(store.names().into_iter().map(|n| {
            let text = store.load(&n);
            entry(n, text)
        }).collect())
    }

    fn load(&self, name: &str) -> Result<String, String> {
        Store::open()?.load(name)
    }

    fn save(&self, name: &str, config: &str, previous: Option<&str>) -> Result<(), String> {
        WgConfig::parse(config).map_err(|e| e.to_string())?;
        Store::open()?.save(name, config, previous)
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        Store::open()?.delete(name)
    }

    fn status(&self) -> Result<ServiceStatus, String> {
        status_from(client::request(&Request::Status))
    }

    fn version(&self) -> Result<String, String> {
        match client::request(&Request::Hello).map_err(|e| e.to_string())? {
            Response::Hello { version, .. } => Ok(version),
            _ => Err("réponse inattendue du service".into()),
        }
    }

    fn connect(&self, name: &str) -> Result<ServiceStatus, String> {
        let config = Store::open()?.load(name)?;
        status_from(client::request(&Request::Connect {
            name: name.to_string(),
            config,
        }))
    }

    fn disconnect(&self, name: Option<&str>) -> Result<ServiceStatus, String> {
        status_from(client::request(&Request::Disconnect {
            name: name.map(str::to_string),
        }))
    }
}

/// Données simulées (développement uniquement).
struct Demo {
    state: Mutex<DemoState>,
}

struct DemoActive {
    name: String,
    full: bool,
    since: u64,
    rx: f64,
    tx: f64,
}

struct DemoState {
    configs: BTreeMap<String, String>,
    active: Vec<DemoActive>,
}

impl Default for Demo {
    fn default() -> Self {
        let mut configs = BTreeMap::new();
        configs.insert(
            "bureau-paris".to_string(),
            "[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 10.8.0.2/32\nDNS = 10.8.0.1\n\n[Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nEndpoint = vpn.paris.example:51820\nAllowedIPs = 0.0.0.0/0, ::/0\nPersistentKeepalive = 25\n".to_string(),
        );
        configs.insert(
            "homelab".to_string(),
            "[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 192.168.50.7/24\n\n[Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nEndpoint = 82.64.12.9:51820\nAllowedIPs = 192.168.1.0/24, 192.168.50.0/24\n".to_string(),
        );
        // `CRYPTONAUTE_DEMO=empty` : aucun tunnel (écran d'accueil).
        if std::env::var("CRYPTONAUTE_DEMO").as_deref() == Ok("empty") {
            configs.clear();
        }
        Demo {
            state: Mutex::new(DemoState {
                configs,
                active: Vec::new(),
            }),
        }
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Demo {
    fn lock(&self) -> std::sync::MutexGuard<'_, DemoState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn snapshot(state: &mut DemoState) -> ServiceStatus {
        let t = now() as f64;
        ServiceStatus {
            tunnels: state
                .active
                .iter_mut()
                .map(|a| {
                    let wave = 1.0 + (t / 4.0).sin();
                    a.rx += pseudo_random(t + a.since as f64) * 900_000.0 * wave;
                    a.tx += pseudo_random(t * 1.7) * 160_000.0;
                    TunnelStatus {
                        name: a.name.clone(),
                        full_tunnel: a.full,
                        connected_since: a.since,
                        rx_bytes: a.rx as u64,
                        tx_bytes: a.tx as u64,
                        last_handshake: Some(now() - 12),
                        endpoint: Some("203.0.113.10:51820".into()),
                    }
                })
                .collect(),
            last_error: None,
        }
    }
}

/// Valeur pseudo-aléatoire dans [0, 1[ (animation des débits simulés).
fn pseudo_random(seed: f64) -> f64 {
    ((seed * 12.9898).sin() * 43758.5453).fract().abs()
}

impl Backend for Demo {
    fn list(&self) -> Result<Vec<TunnelEntry>, String> {
        let state = self.lock();
        Ok(state.configs.iter().map(|(n, c)| entry(n.clone(), Ok(c.clone()))).collect())
    }

    fn load(&self, name: &str) -> Result<String, String> {
        self.lock().configs.get(name).cloned().ok_or_else(|| format!("tunnel « {name} » introuvable"))
    }

    fn save(&self, name: &str, config: &str, previous: Option<&str>) -> Result<(), String> {
        WgConfig::parse(config).map_err(|e| e.to_string())?;
        let mut state = self.lock();
        if let Some(prev) = previous {
            state.configs.remove(prev);
        }
        state.configs.insert(name.to_string(), config.to_string());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        self.lock().configs.remove(name);
        Ok(())
    }

    fn status(&self) -> Result<ServiceStatus, String> {
        Ok(Self::snapshot(&mut self.lock()))
    }

    fn version(&self) -> Result<String, String> {
        Ok(format!("{} (démo)", env!("CARGO_PKG_VERSION")))
    }

    fn connect(&self, name: &str) -> Result<ServiceStatus, String> {
        std::thread::sleep(std::time::Duration::from_millis(1400));
        let mut state = self.lock();
        let text = state.configs.get(name).cloned().ok_or("tunnel introuvable")?;
        let full = WgConfig::parse(&text).map_err(|e| e.to_string())?.is_full_tunnel();
        state.active.retain(|a| a.name != name && !(full && a.full));
        state.active.push(DemoActive {
            name: name.to_string(),
            full,
            since: now(),
            rx: 0.0,
            tx: 0.0,
        });
        Ok(Self::snapshot(&mut state))
    }

    fn disconnect(&self, name: Option<&str>) -> Result<ServiceStatus, String> {
        std::thread::sleep(std::time::Duration::from_millis(600));
        let mut state = self.lock();
        state.active.retain(|a| name.is_some_and(|n| a.name != n));
        Ok(Self::snapshot(&mut state))
    }
}

//! Gestion du tunnel actif via WireGuardNT (pilote noyau officiel).
//!
//! Un seul tunnel est actif à la fois. L'adaptateur réseau est supprimé
//! automatiquement par le pilote lorsque son handle est fermé, ce qui retire
//! aussi les adresses et routes associées.

use std::net::{SocketAddr, ToSocketAddrs};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use cryptonaute_common::config::{validate_tunnel_name, Endpoint, WgConfig};
use cryptonaute_common::ipc::{TunnelState, TunnelStatus};
use wireguard_nt::{Adapter, SetInterface, SetPeer, Wireguard};

use super::net;

const ADAPTER_POOL: &str = "Cryptonaute";

struct Active {
    name: String,
    adapter: Adapter,
    luid: u64,
    since: SystemTime,
    endpoint: SocketAddr,
}

#[derive(Default)]
struct Inner {
    driver: Option<Wireguard>,
    active: Option<Active>,
    last_error: Option<String>,
}

pub struct TunnelManager {
    dll_path: PathBuf,
    inner: Mutex<Inner>,
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Résout un endpoint, en privilégiant IPv4 (plus souvent disponible).
fn resolve(endpoint: &Endpoint) -> anyhow::Result<SocketAddr> {
    let addrs: Vec<SocketAddr> = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .with_context(|| format!("résolution DNS de {} impossible", endpoint.host))?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .copied()
        .ok_or_else(|| anyhow!("aucune adresse trouvée pour {}", endpoint.host))
}

impl TunnelManager {
    pub fn new(dll_path: PathBuf) -> Self {
        TunnelManager {
            dll_path,
            inner: Mutex::new(Inner::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> TunnelStatus {
        let inner = self.lock();
        let Some(active) = &inner.active else {
            return TunnelStatus::disconnected(inner.last_error.clone());
        };
        let (mut rx, mut tx, mut handshake) = (0u64, 0u64, None::<u64>);
        // `get_config` utilise des assertions internes : on isole un éventuel panic.
        if let Ok(cfg) = catch_unwind(AssertUnwindSafe(|| active.adapter.get_config())) {
            for peer in &cfg.peers {
                rx += peer.rx_bytes;
                tx += peer.tx_bytes;
                if let Some(t) = peer.last_handshake.map(unix_secs) {
                    handshake = Some(handshake.map_or(t, |h| h.max(t)));
                }
            }
        }
        TunnelStatus {
            state: TunnelState::Connected,
            tunnel: Some(active.name.clone()),
            connected_since: Some(unix_secs(active.since)),
            rx_bytes: rx,
            tx_bytes: tx,
            last_handshake: handshake,
            endpoint: Some(active.endpoint.to_string()),
            last_error: None,
        }
    }

    pub fn connect(&self, name: &str, config_text: &str) -> Result<TunnelStatus, String> {
        validate_tunnel_name(name)?;
        // Le service revalide toujours la configuration : le client n'est pas digne de confiance.
        let cfg = WgConfig::parse(config_text).map_err(|e| e.to_string())?;

        let mut inner = self.lock();
        Self::teardown(&mut inner);
        inner.last_error = None;

        match self.bring_up(&mut inner, name, &cfg) {
            Ok(active) => {
                log::info!("tunnel « {name} » actif (endpoint {})", active.endpoint);
                inner.active = Some(active);
                drop(inner);
                Ok(self.status())
            }
            Err(e) => {
                let msg = format!("{e:#}");
                log::error!("échec de l'activation de « {name} » : {msg}");
                inner.last_error = Some(msg.clone());
                Err(msg)
            }
        }
    }

    pub fn disconnect(&self) -> TunnelStatus {
        let mut inner = self.lock();
        Self::teardown(&mut inner);
        TunnelStatus::disconnected(None)
    }

    fn driver(&self, inner: &mut Inner) -> anyhow::Result<Wireguard> {
        if let Some(d) = &inner.driver {
            return Ok(d.clone());
        }
        // SAFETY: wireguard.dll est chargée depuis le dossier d'installation, qui
        // n'est modifiable que par les administrateurs.
        let driver = unsafe { wireguard_nt::load_from_path(&self.dll_path) }
            .with_context(|| format!("chargement de {} impossible", self.dll_path.display()))?;
        inner.driver = Some(driver.clone());
        Ok(driver)
    }

    fn bring_up(&self, inner: &mut Inner, name: &str, cfg: &WgConfig) -> anyhow::Result<Active> {
        let driver = self.driver(inner)?;

        let mut peers = Vec::with_capacity(cfg.peers.len());
        for p in &cfg.peers {
            let endpoint = p
                .endpoint
                .as_ref()
                .ok_or_else(|| anyhow!("pair sans endpoint"))?;
            peers.push(SetPeer {
                public_key: Some(p.public_key.0),
                preshared_key: p.preshared_key.as_ref().map(|k| k.0),
                keep_alive: p.persistent_keepalive,
                endpoint: resolve(endpoint)?,
                allowed_ips: p.allowed_ips.clone(),
            });
        }
        let first_endpoint = peers[0].endpoint;

        let mut iface = SetInterface {
            listen_port: cfg.interface.listen_port,
            public_key: None,
            private_key: Some(cfg.interface.private_key.0),
            peers,
        };

        let adapter = Adapter::create(&driver, ADAPTER_POOL, name, None)
            .context("création de l'adaptateur WireGuard impossible")?;
        let result = adapter.set_config(&iface);
        // Efface les copies des clés secrètes dès qu'elles ont été transmises au pilote.
        if let Some(k) = iface.private_key.as_mut() {
            k.fill(0);
        }
        for p in &mut iface.peers {
            if let Some(k) = p.preshared_key.as_mut() {
                k.fill(0);
            }
        }
        result.context("configuration de l'interface WireGuard refusée")?;

        let luid = adapter.get_luid();
        net::configure(luid, cfg)?;
        adapter.up().context("activation de l'adaptateur impossible")?;
        // L'adaptateur est supprimé (avec adresses et routes) si une étape échoue,
        // car `adapter` est libéré en sortie de fonction.

        Ok(Active {
            name: name.to_string(),
            adapter,
            luid,
            since: SystemTime::now(),
            endpoint: first_endpoint,
        })
    }

    fn teardown(inner: &mut Inner) {
        if let Some(active) = inner.active.take() {
            net::clear_dns(active.luid);
            if let Err(e) = active.adapter.down() {
                log::warn!("arrêt de l'adaptateur : {e}");
            }
            drop(active.adapter);
            log::info!("tunnel « {} » arrêté", active.name);
        }
    }
}

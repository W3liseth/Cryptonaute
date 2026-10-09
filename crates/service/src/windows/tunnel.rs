//! Tunnels WireGuardNT (pilote noyau officiel), un adaptateur réseau par tunnel.
//!
//! L'adaptateur est supprimé automatiquement par le pilote lorsque son handle
//! est fermé, ce qui retire aussi les adresses et routes associées.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, Context};
use cryptonaute_common::config::{Endpoint, WgConfig};
use wireguard_nt::{Adapter, SetInterface, SetPeer, Wireguard};

use super::net;
use crate::manager::{Active, Backend, Manager, Stats};

const ADAPTER_POOL: &str = "Cryptonaute";

pub type TunnelManager = Manager<WireGuardNt>;

pub struct WireGuardNt {
    dll_path: PathBuf,
    driver: Mutex<Option<Wireguard>>,
}

pub struct Tunnel {
    adapter: Adapter,
    luid: u64,
    endpoint: SocketAddr,
    /// Routes vers les endpoints posées par Cryptonaute (tunnel partiel).
    pinned: Vec<net::PinnedRoute>,
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

impl WireGuardNt {
    pub fn new(dll_path: PathBuf) -> Self {
        WireGuardNt {
            dll_path,
            driver: Mutex::new(None),
        }
    }

    fn driver(&self) -> anyhow::Result<Wireguard> {
        let mut driver = self.driver.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(d) = &*driver {
            return Ok(d.clone());
        }
        // SAFETY: wireguard.dll est chargée depuis le dossier d'installation, qui
        // n'est modifiable que par les administrateurs.
        let loaded = unsafe { wireguard_nt::load_from_path(&self.dll_path) }
            .with_context(|| format!("chargement de {} impossible", self.dll_path.display()))?;
        *driver = Some(loaded.clone());
        Ok(loaded)
    }
}

impl Backend for WireGuardNt {
    type Tunnel = Tunnel;

    fn up(&self, name: &str, cfg: &WgConfig, others: &[Active<Tunnel>]) -> anyhow::Result<Tunnel> {
        let driver = self.driver()?;

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
        let endpoints: Vec<SocketAddr> = peers.iter().map(|p| p.endpoint).collect();

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
        let full = cfg.is_full_tunnel();
        net::configure(luid, cfg, full)?;
        adapter.up().context("activation de l'adaptateur impossible")?;
        // L'adaptateur est supprimé (avec adresses et routes) si une étape échoue,
        // car `adapter` est libéré en sortie de fonction.

        // WireGuardNT n'exclut que son propre adaptateur pour joindre ses pairs : sans
        // route dédiée, ceux d'un tunnel partiel passeraient par la route par défaut
        // d'un tunnel complet actif. On les épingle sur la passerelle physique.
        let pinned = if full {
            Vec::new()
        } else {
            // Route déjà posée pour un autre tunnel vers le même serveur : partagée.
            let shared: Vec<net::PinnedRoute> = others
                .iter()
                .flat_map(|a| &a.tunnel.pinned)
                .filter(|r| endpoints.iter().any(|e| e.ip() == r.ip))
                .cloned()
                .collect();
            let missing: Vec<IpAddr> = endpoints
                .iter()
                .map(SocketAddr::ip)
                .filter(|ip| !shared.iter().any(|r| r.ip == *ip))
                .collect();
            let ours: Vec<u64> = others.iter().map(|a| a.tunnel.luid).chain([luid]).collect();
            shared.into_iter().chain(net::pin_endpoints(&missing, &ours)).collect()
        };

        Ok(Tunnel {
            adapter,
            luid,
            endpoint: endpoints[0],
            pinned,
        })
    }

    fn down(&self, tunnel: Tunnel, _cfg: &WgConfig, remaining: &[Active<Tunnel>]) {
        let still_used = |r: &&net::PinnedRoute| {
            remaining.iter().any(|a| a.tunnel.pinned.iter().any(|o| o.ip == r.ip))
        };
        let unused: Vec<net::PinnedRoute> =
            tunnel.pinned.iter().filter(|r| !still_used(r)).cloned().collect();
        net::unpin(&unused);
        net::clear_dns(tunnel.luid);
        if let Err(e) = tunnel.adapter.down() {
            log::warn!("arrêt de l'adaptateur : {e}");
        }
        drop(tunnel.adapter);
    }

    fn stats(&self, tunnel: &Tunnel) -> Stats {
        let mut stats = Stats {
            endpoint: Some(tunnel.endpoint.to_string()),
            ..Stats::default()
        };
        // `get_config` utilise des assertions internes : on isole un éventuel panic.
        if let Ok(cfg) = catch_unwind(AssertUnwindSafe(|| tunnel.adapter.get_config())) {
            for peer in &cfg.peers {
                stats.rx_bytes += peer.rx_bytes;
                stats.tx_bytes += peer.tx_bytes;
                if let Some(t) = peer.last_handshake.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()) {
                    let t = t.as_secs();
                    stats.last_handshake = Some(stats.last_handshake.map_or(t, |h| h.max(t)));
                }
            }
        }
        stats
    }
}

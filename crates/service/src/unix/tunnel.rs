//! Gestion du tunnel actif sous Linux et macOS via `defguard_wireguard_rs` :
//! - Linux : module noyau WireGuard (netlink), avec repli sur boringtun
//!   (implémentation en espace utilisateur) si le module est absent ;
//! - macOS : boringtun sur une interface `utunN`.
//!
//! La bibliothèque reproduit le comportement de `wg-quick` pour les routes
//! (y compris le tunnel complet) et configure le DNS (systemd-resolved ou
//! resolvconf sous Linux, `networksetup` sous macOS). Un seul tunnel est actif
//! à la fois.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
#[cfg(target_os = "linux")]
use defguard_wireguard_rs::Kernel;
use defguard_wireguard_rs::key::Key as WgKey;
use defguard_wireguard_rs::net::IpAddrMask;
use defguard_wireguard_rs::peer::Peer;
use defguard_wireguard_rs::{InterfaceConfiguration, Userspace, WGApi, WireguardInterfaceApi};
use cryptonaute_common::config::{validate_tunnel_name, Endpoint, WgConfig};
use cryptonaute_common::ipc::{TunnelState, TunnelStatus};

const DEFAULT_MTU: u32 = 1420;

type Api = Box<dyn WireguardInterfaceApi + Send>;

struct Active {
    name: String,
    ifname: String,
    api: Api,
    endpoints: Vec<SocketAddr>,
    since: SystemTime,
}

#[derive(Default)]
struct Inner {
    active: Option<Active>,
    last_error: Option<String>,
}

pub struct TunnelManager {
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

/// Nom de l'interface réseau à créer.
#[cfg(target_os = "linux")]
fn interface_name() -> anyhow::Result<String> {
    Ok("cryptonaute0".to_string())
}

/// Sous macOS, les interfaces de tunnel doivent s'appeler `utunN` : on prend
/// le premier numéro libre.
#[cfg(not(target_os = "linux"))]
fn interface_name() -> anyhow::Result<String> {
    (0..256)
        .map(|n| format!("utun{n}"))
        .find(|name| {
            let c = std::ffi::CString::new(name.as_str()).expect("nom sans octet nul");
            // SAFETY: chaîne C valide ; renvoie 0 si l'interface n'existe pas.
            unsafe { libc::if_nametoindex(c.as_ptr()) == 0 }
        })
        .ok_or_else(|| anyhow!("aucune interface utun disponible"))
}

/// Crée l'interface : WireGuard noyau sous Linux si possible, sinon boringtun.
fn create_api(ifname: &str) -> anyhow::Result<Api> {
    #[cfg(target_os = "linux")]
    {
        let mut kernel = WGApi::<Kernel>::new(ifname)?;
        // Interface laissée par une exécution précédente interrompue.
        if kernel.remove_interface().is_err() {
            force_delete_link(ifname);
        }
        match kernel.create_interface() {
            Ok(()) => return Ok(Box::new(kernel)),
            Err(e) => log::warn!("WireGuard noyau indisponible ({e}) : repli sur boringtun"),
        }
    }
    let mut user = WGApi::<Userspace>::new(ifname)?;
    user.create_interface()
        .context("création de l'interface WireGuard impossible")?;
    Ok(Box::new(user))
}

fn net_mask(net: &ipnet::IpNet, host_bits: bool) -> IpAddrMask {
    let addr = if host_bits { net.addr() } else { net.network() };
    IpAddrMask::new(addr, net.prefix_len())
}

impl TunnelManager {
    pub fn new() -> Self {
        TunnelManager {
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
        if let Ok(host) = active.api.read_interface_data() {
            for peer in host.peers.values() {
                rx += peer.rx_bytes;
                tx += peer.tx_bytes;
                if let Some(t) = peer.last_handshake.map(unix_secs).filter(|&t| t > 0) {
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
            endpoint: active.endpoints.first().map(ToString::to_string),
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

        match bring_up(name, &cfg) {
            Ok(active) => {
                log::info!("tunnel « {name} » actif sur {}", active.ifname);
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

    fn teardown(inner: &mut Inner) {
        if let Some(active) = inner.active.take() {
            shutdown(&active.api, &active.ifname, &active.endpoints);
            log::info!("tunnel « {} » arrêté", active.name);
        }
    }
}

/// Retire les routes vers les endpoints puis l'interface (qui emporte DNS et routes).
fn shutdown(api: &Api, ifname: &str, endpoints: &[SocketAddr]) {
    #[cfg(target_os = "linux")]
    super::firewall::remove();
    for ep in endpoints {
        let _ = api.remove_endpoint_routing(&ep.to_string());
    }
    if let Err(e) = api.remove_interface() {
        log::warn!("suppression de l'interface : {e}");
        force_delete_link(ifname);
    }
}

/// `remove_interface` s'interrompt si l'effacement du DNS échoue (aucun
/// gestionnaire DNS sur la machine, par exemple) : on supprime alors le lien
/// directement pour ne jamais laisser l'interface en place.
#[cfg(target_os = "linux")]
fn force_delete_link(ifname: &str) {
    if !std::path::Path::new("/sys/class/net").join(ifname).exists() {
        return;
    }
    match std::process::Command::new("ip").args(["link", "delete", "dev", ifname]).status() {
        Ok(s) if s.success() => log::info!("interface {ifname} supprimée"),
        Ok(s) => log::error!("suppression de {ifname} impossible ({s})"),
        Err(e) => log::error!("suppression de {ifname} impossible : {e}"),
    }
}

/// Sous macOS, l'interface `utun` disparaît avec le périphérique boringtun,
/// libéré en même temps que l'API.
#[cfg(not(target_os = "linux"))]
fn force_delete_link(_ifname: &str) {}

fn bring_up(name: &str, cfg: &WgConfig) -> anyhow::Result<Active> {
    let mut peers = Vec::with_capacity(cfg.peers.len());
    let mut endpoints = Vec::with_capacity(cfg.peers.len());
    for p in &cfg.peers {
        let endpoint = resolve(p.endpoint.as_ref().ok_or_else(|| anyhow!("pair sans endpoint"))?)?;
        let mut peer = Peer::new(WgKey::new(p.public_key.0));
        peer.preshared_key = p.preshared_key.as_ref().map(|k| WgKey::new(k.0));
        peer.endpoint = Some(endpoint);
        peer.persistent_keepalive_interval = p.persistent_keepalive;
        peer.allowed_ips = p.allowed_ips.iter().map(|n| net_mask(n, false)).collect();
        endpoints.push(endpoint);
        peers.push(peer);
    }

    let ifname = interface_name()?;
    let api = create_api(&ifname)?;

    let setup = || -> anyhow::Result<()> {
        // Tunnel complet sous Linux : les paquets WireGuard sont marqués (fwmark) pour
        // échapper à la table du tunnel, et la marque doit être restaurée sur les
        // réponses (règles nftables, comme `wg-quick`). Les règles sont posées avant
        // la configuration du pair, sinon la première poignée de main serait perdue.
        #[cfg(target_os = "linux")]
        let fwmark = if cfg.is_full_tunnel() {
            let mark = super::firewall::free_fwmark();
            super::firewall::apply(&ifname, mark, &cfg.interface.addresses)
                .context("règles nftables du tunnel complet")?;
            Some(mark)
        } else {
            None
        };
        #[cfg(not(target_os = "linux"))]
        let fwmark = None;

        let iface = InterfaceConfiguration {
            name: ifname.clone(),
            prvkey: cfg.interface.private_key.to_base64(),
            addresses: cfg.interface.addresses.iter().map(|a| net_mask(a, true)).collect(),
            port: cfg.interface.listen_port.unwrap_or(0),
            peers: peers.clone(),
            mtu: Some(cfg.interface.mtu.map(u32::from).unwrap_or(DEFAULT_MTU)),
            fwmark,
        };
        api.configure_interface(&iface)
            .context("configuration de l'interface WireGuard refusée")?;
        api.configure_peer_routing(&peers)
            .context("configuration des routes impossible")?;
        if !cfg.interface.dns_servers.is_empty() {
            let servers: Vec<IpAddr> = cfg.interface.dns_servers.clone();
            let search: Vec<&str> = cfg.interface.dns_search.iter().map(String::as_str).collect();
            api.configure_dns(&servers, &search)
                .context("configuration DNS impossible")?;
        }
        Ok(())
    };

    if let Err(e) = setup() {
        shutdown(&api, &ifname, &endpoints);
        return Err(e);
    }

    Ok(Active {
        name: name.to_string(),
        ifname,
        api,
        endpoints,
        since: SystemTime::now(),
    })
}

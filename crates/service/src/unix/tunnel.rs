//! Tunnels sous Linux et macOS via `defguard_wireguard_rs`, une interface par tunnel :
//! - Linux : module noyau WireGuard (netlink), avec repli sur boringtun
//!   (implémentation en espace utilisateur) si le module est absent ;
//! - macOS : boringtun sur une interface `utunN`.
//!
//! La bibliothèque reproduit le comportement de `wg-quick` pour les routes (y
//! compris le tunnel complet et les routes d'hôte vers les endpoints, qui
//! empêchent un tunnel partiel de passer par un tunnel complet) et configure le DNS
//! (systemd-resolved ou resolvconf sous Linux, `networksetup` sous macOS).

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use anyhow::{anyhow, Context};
use cryptonaute_common::config::{Endpoint, WgConfig};
use defguard_wireguard_rs::dns::DnsConfig;
#[cfg(target_os = "linux")]
use defguard_wireguard_rs::Kernel;
use defguard_wireguard_rs::key::Key as WgKey;
use defguard_wireguard_rs::net::IpAddrMask;
use defguard_wireguard_rs::peer::Peer;
use defguard_wireguard_rs::{InterfaceConfiguration, Userspace, WGApi, WireguardInterfaceApi};

use crate::manager::{Active, Backend, Manager, Stats};

const DEFAULT_MTU: u32 = 1420;

type Api = Box<dyn WireguardInterfaceApi + Send>;

pub type TunnelManager = Manager<Defguard>;

pub struct Defguard;

pub struct Tunnel {
    ifname: String,
    api: Api,
    endpoints: Vec<SocketAddr>,
    /// Endpoints dont la route d'hôte (posée par `configure_peer_routing`) doit
    /// être retirée avec le dernier tunnel qui l'utilise.
    pinned: Vec<SocketAddr>,
}

/// Vrai si une route d'hôte vers `ip` existe déjà dans la table principale
/// (route de l'administrateur, à ne pas retirer).
#[cfg(target_os = "linux")]
fn host_route_exists(ip: IpAddr) -> bool {
    let (family, prefix) = if ip.is_ipv4() { ("-4", 32) } else { ("-6", 128) };
    std::process::Command::new("ip")
        .args([family, "route", "show", "table", "main", "exact", &format!("{ip}/{prefix}")])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Sous macOS, la bibliothèque remplace toute route d'hôte existante vers l'endpoint.
#[cfg(not(target_os = "linux"))]
fn host_route_exists(_ip: IpAddr) -> bool {
    false
}

/// Retire la route d'hôte vers un endpoint.
#[cfg(target_os = "linux")]
fn remove_endpoint_route(_api: &Api, endpoint: &SocketAddr) {
    let ip = endpoint.ip();
    let prefix = if ip.is_ipv4() { 32 } else { 128 };
    let _ = std::process::Command::new("ip")
        .args(["route", "del", &format!("{ip}/{prefix}"), "table", "main"])
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(target_os = "linux"))]
fn remove_endpoint_route(api: &Api, endpoint: &SocketAddr) {
    let _ = api.remove_endpoint_routing(&endpoint.to_string());
}

/// Endpoints de `pinned` qu'aucun des tunnels `others` ne joint.
fn unshared(pinned: &[SocketAddr], others: &[Active<Tunnel>]) -> Vec<SocketAddr> {
    pinned
        .iter()
        .filter(|ep| !others.iter().any(|a| a.tunnel.pinned.iter().any(|o| o.ip() == ep.ip())))
        .copied()
        .collect()
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

#[cfg(target_os = "linux")]
const IFNAME_PREFIX: &str = "cryptonaute";

/// Nom de l'interface à créer : `cryptonaute0`, `cryptonaute1`… (premier libre).
#[cfg(target_os = "linux")]
fn interface_name() -> anyhow::Result<String> {
    (0..64)
        .map(|n| format!("{IFNAME_PREFIX}{n}"))
        .find(|name| !std::path::Path::new("/sys/class/net").join(name).exists())
        .ok_or_else(|| anyhow!("aucun nom d'interface disponible"))
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

/// Supprime les interfaces laissées par une exécution précédente interrompue.
#[cfg(target_os = "linux")]
pub fn remove_stale_interfaces() {
    let Ok(entries) = std::fs::read_dir("/sys/class/net") else { return };
    for name in entries.flatten().filter_map(|e| e.file_name().into_string().ok()) {
        if name.starts_with(IFNAME_PREFIX) {
            force_delete_link(&name);
        }
    }
}

/// Crée l'interface : WireGuard noyau sous Linux si possible, sinon boringtun.
fn create_api(ifname: &str) -> anyhow::Result<Api> {
    #[cfg(target_os = "linux")]
    {
        let mut kernel = WGApi::<Kernel>::new(ifname)?;
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

/// Réglages DNS d'un tunnel. Ceux d'un tunnel complet répondent à toutes les
/// requêtes ; ceux d'un tunnel partiel avec domaines de recherche ne répondent
/// qu'à ces domaines (systemd-resolved), comme avec `wg-quick`.
fn apply_dns(api: &Api, cfg: &WgConfig) -> anyhow::Result<()> {
    let servers: &[IpAddr] = &cfg.interface.dns_servers;
    if servers.is_empty() {
        return Ok(());
    }
    let search: Vec<&str> = cfg.interface.dns_search.iter().map(String::as_str).collect();
    let dns = DnsConfig {
        servers,
        search_domains: &search,
        routing_domains: &[],
        default_route: cfg.is_full_tunnel() || search.is_empty(),
    };
    api.set_dns(&dns).context("configuration DNS impossible")
}

impl Backend for Defguard {
    type Tunnel = Tunnel;

    fn up(&self, _name: &str, cfg: &WgConfig, others: &[Active<Tunnel>]) -> anyhow::Result<Tunnel> {
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

        // Routes d'hôte vers les endpoints : à nous si elles n'existaient pas encore,
        // ou si un autre tunnel actif les a posées (même serveur).
        let mut pinned: Vec<SocketAddr> = Vec::new();
        for ep in &endpoints {
            let shared = others.iter().any(|a| a.tunnel.pinned.iter().any(|o| o.ip() == ep.ip()));
            if !pinned.iter().any(|p| p.ip() == ep.ip()) && (shared || !host_route_exists(ep.ip())) {
                pinned.push(*ep);
            }
        }

        let ifname = interface_name()?;
        let api = create_api(&ifname)?;
        let full = cfg.is_full_tunnel();

        let setup = || -> anyhow::Result<()> {
            // Tunnel complet sous Linux : les paquets WireGuard sont marqués (fwmark) pour
            // échapper à la table du tunnel, et la marque doit être restaurée sur les
            // réponses (règles nftables, comme `wg-quick`). Les règles sont posées avant
            // la configuration du pair, sinon la première poignée de main serait perdue.
            #[cfg(target_os = "linux")]
            let fwmark = if full {
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
            // Sous macOS, le DNS est global : il est appliqué par `changed`.
            #[cfg(target_os = "linux")]
            apply_dns(&api, cfg)?;
            Ok(())
        };

        if let Err(e) = setup() {
            shutdown(&api, &ifname, &unshared(&pinned, others), full);
            return Err(e);
        }
        log::info!("interface {ifname} créée");
        Ok(Tunnel { ifname, api, endpoints, pinned })
    }

    fn down(&self, tunnel: Tunnel, cfg: &WgConfig, remaining: &[Active<Tunnel>]) {
        let unpin = unshared(&tunnel.pinned, remaining);
        shutdown(&tunnel.api, &tunnel.ifname, &unpin, cfg.is_full_tunnel());
    }

    fn stats(&self, tunnel: &Tunnel) -> Stats {
        let mut stats = Stats {
            endpoint: tunnel.endpoints.first().map(ToString::to_string),
            ..Stats::default()
        };
        if let Ok(host) = tunnel.api.read_interface_data() {
            for peer in host.peers.values() {
                stats.rx_bytes += peer.rx_bytes;
                stats.tx_bytes += peer.tx_bytes;
                let handshake = peer
                    .last_handshake
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .filter(|&t| t > 0);
                if let Some(t) = handshake {
                    stats.last_handshake = Some(stats.last_handshake.map_or(t, |h| h.max(t)));
                }
            }
        }
        stats
    }

    /// Sous macOS, les serveurs DNS sont globaux (`networksetup`) et effacés à la
    /// suppression de chaque interface : on applique ceux du tunnel complet actif,
    /// sinon ceux du dernier tunnel activé qui en définit.
    #[cfg(target_os = "macos")]
    fn changed(&self, active: &[Active<Tunnel>]) {
        let with_dns = |a: &&Active<Tunnel>| !a.cfg.interface.dns_servers.is_empty();
        let source = active
            .iter()
            .filter(with_dns)
            .find(|a| a.cfg.is_full_tunnel())
            .or_else(|| active.iter().rev().find(with_dns));
        if let Some(a) = source {
            if let Err(e) = apply_dns(&a.tunnel.api, &a.cfg) {
                log::warn!("DNS de « {} » : {e:#}", a.name);
            }
        }
    }
}

/// Retire les routes vers les endpoints `unpin`, les règles du tunnel complet puis
/// l'interface (qui emporte DNS et routes).
fn shutdown(api: &Api, ifname: &str, unpin: &[SocketAddr], full: bool) {
    #[cfg(target_os = "linux")]
    if full {
        super::firewall::remove();
    }
    #[cfg(not(target_os = "linux"))]
    let _ = full;
    for ep in unpin {
        remove_endpoint_route(api, ep);
    }
    if let Err(e) = api.remove_interface() {
        log::warn!("suppression de l'interface {ifname} : {e}");
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

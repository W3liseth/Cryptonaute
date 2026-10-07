//! Configuration IP de l'interface WireGuard via l'API IP Helper :
//! adresses, routes vers les `AllowedIPs`, MTU, métrique et DNS.

use std::collections::BTreeSet;
use std::net::IpAddr;

use anyhow::bail;
use ipnet::IpNet;
use cryptonaute_common::config::WgConfig;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{ERROR_OBJECT_ALREADY_EXISTS, NO_ERROR, WIN32_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    ConvertInterfaceLuidToGuid, CreateIpForwardEntry2, CreateUnicastIpAddressEntry,
    GetIpInterfaceEntry, InitializeIpForwardEntry, InitializeIpInterfaceEntry,
    InitializeUnicastIpAddressEntry, SetInterfaceDnsSettings, SetIpInterfaceEntry,
    DNS_INTERFACE_SETTINGS, DNS_INTERFACE_SETTINGS_VERSION1, DNS_SETTING_IPV6,
    DNS_SETTING_NAMESERVER, DNS_SETTING_SEARCHLIST, MIB_IPFORWARD_ROW2, MIB_IPINTERFACE_ROW,
    MIB_UNICASTIPADDRESS_ROW,
};
use windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows_sys::Win32::Networking::WinSock::{
    IpDadStatePreferred, RouterDiscoveryDisabled, ADDRESS_FAMILY, AF_INET, AF_INET6, IN6_ADDR,
    IN6_ADDR_0, IN_ADDR, IN_ADDR_0, SOCKADDR_INET,
};

const DEFAULT_MTU: u32 = 1420;

fn os_err(context: &str, code: WIN32_ERROR) -> anyhow::Error {
    anyhow::anyhow!(
        "{context} : {}",
        std::io::Error::from_raw_os_error(code as i32)
    )
}

fn luid(value: u64) -> NET_LUID_LH {
    NET_LUID_LH { Value: value }
}

fn family_of(ip: &IpAddr) -> ADDRESS_FAMILY {
    if ip.is_ipv4() {
        AF_INET
    } else {
        AF_INET6
    }
}

fn write_sockaddr(dst: &mut SOCKADDR_INET, ip: IpAddr) {
    match ip {
        IpAddr::V4(v4) => {
            dst.si_family = AF_INET;
            dst.Ipv4.sin_addr = IN_ADDR {
                S_un: IN_ADDR_0 {
                    S_addr: u32::from_ne_bytes(v4.octets()),
                },
            };
        }
        IpAddr::V6(v6) => {
            dst.si_family = AF_INET6;
            dst.Ipv6.sin6_addr = IN6_ADDR {
                u: IN6_ADDR_0 { Byte: v6.octets() },
            };
        }
    }
}

fn add_address(luid_val: u64, net: &IpNet) -> anyhow::Result<()> {
    // SAFETY: structure initialisée par l'API puis complétée champ par champ.
    unsafe {
        let mut row: MIB_UNICASTIPADDRESS_ROW = std::mem::zeroed();
        InitializeUnicastIpAddressEntry(&mut row);
        row.InterfaceLuid = luid(luid_val);
        row.OnLinkPrefixLength = net.prefix_len();
        row.DadState = IpDadStatePreferred;
        write_sockaddr(&mut row.Address, net.addr());
        let err = CreateUnicastIpAddressEntry(&row);
        if err != NO_ERROR && err != ERROR_OBJECT_ALREADY_EXISTS {
            return Err(os_err(&format!("ajout de l'adresse {net}"), err));
        }
    }
    Ok(())
}

fn add_route(luid_val: u64, net: &IpNet) -> Result<(), WIN32_ERROR> {
    // SAFETY: structure initialisée par l'API ; passerelle nulle = route « on-link ».
    unsafe {
        let mut row: MIB_IPFORWARD_ROW2 = std::mem::zeroed();
        InitializeIpForwardEntry(&mut row);
        row.InterfaceLuid = luid(luid_val);
        row.Metric = 0;
        write_sockaddr(&mut row.DestinationPrefix.Prefix, net.network());
        row.DestinationPrefix.PrefixLength = net.prefix_len();
        row.NextHop.si_family = family_of(&net.addr());
        let err = CreateIpForwardEntry2(&row);
        if err != NO_ERROR && err != ERROR_OBJECT_ALREADY_EXISTS {
            return Err(err);
        }
    }
    Ok(())
}

/// Fixe MTU et métrique 0 (priorité maximale) pour une famille d'adresses.
fn set_interface_params(luid_val: u64, family: ADDRESS_FAMILY, mtu: u32) -> Result<(), WIN32_ERROR> {
    // SAFETY: lecture-modification-écriture de la ligne d'interface fournie par l'API.
    unsafe {
        let mut row: MIB_IPINTERFACE_ROW = std::mem::zeroed();
        InitializeIpInterfaceEntry(&mut row);
        row.InterfaceLuid = luid(luid_val);
        row.Family = family;
        let err = GetIpInterfaceEntry(&mut row);
        if err != NO_ERROR {
            return Err(err);
        }
        row.UseAutomaticMetric = 0;
        row.Metric = 0;
        row.NlMtu = mtu;
        row.SitePrefixLength = 0;
        if family == AF_INET6 {
            row.RouterDiscoveryBehavior = RouterDiscoveryDisabled;
            row.DadTransmits = 0;
            row.ManagedAddressConfigurationSupported = 0;
            row.OtherStatefulConfigurationSupported = 0;
        }
        let err = SetIpInterfaceEntry(&mut row);
        if err != NO_ERROR {
            return Err(err);
        }
    }
    Ok(())
}

fn interface_guid(luid_val: u64) -> anyhow::Result<GUID> {
    let mut guid: GUID = unsafe { std::mem::zeroed() };
    let err = unsafe { ConvertInterfaceLuidToGuid(&luid(luid_val), &mut guid) };
    if err != NO_ERROR {
        return Err(os_err("ConvertInterfaceLuidToGuid", err));
    }
    Ok(guid)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Applique serveurs DNS et domaines de recherche (Windows 10 2004 et ultérieur).
fn set_dns(guid: GUID, ipv6: bool, servers: &[IpAddr], search: Option<&[String]>) -> Result<(), WIN32_ERROR> {
    let mut name_server = wide(
        &servers
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(","),
    );
    let mut search_list = wide(&search.unwrap_or_default().join(","));
    let mut flags = DNS_SETTING_NAMESERVER as u64;
    if ipv6 {
        flags |= DNS_SETTING_IPV6 as u64;
    }
    if search.is_some() {
        flags |= DNS_SETTING_SEARCHLIST as u64;
    }
    // SAFETY: les tampons UTF-16 restent vivants pendant l'appel.
    unsafe {
        let mut settings: DNS_INTERFACE_SETTINGS = std::mem::zeroed();
        settings.Version = DNS_INTERFACE_SETTINGS_VERSION1;
        settings.Flags = flags;
        settings.NameServer = name_server.as_mut_ptr();
        settings.SearchList = search_list.as_mut_ptr();
        let err = SetInterfaceDnsSettings(guid, &settings);
        if err != NO_ERROR {
            return Err(err);
        }
    }
    Ok(())
}

pub fn configure(luid_val: u64, cfg: &WgConfig) -> anyhow::Result<()> {
    let iface = &cfg.interface;
    let mtu = iface.mtu.map(u32::from).unwrap_or(DEFAULT_MTU);
    let has_v4 = iface.addresses.iter().any(|a| a.addr().is_ipv4());
    let has_v6 = iface.addresses.iter().any(|a| a.addr().is_ipv6());

    for (family, present) in [(AF_INET, has_v4), (AF_INET6, has_v6)] {
        if let Err(err) = set_interface_params(luid_val, family, mtu) {
            if present {
                return Err(os_err("paramétrage de l'interface", err));
            }
            log::warn!("famille {family} non configurable : {}", std::io::Error::from_raw_os_error(err as i32));
        }
    }

    for addr in &iface.addresses {
        add_address(luid_val, addr)?;
    }

    let routes: BTreeSet<IpNet> = cfg
        .peers
        .iter()
        .flat_map(|p| p.allowed_ips.iter().copied())
        .collect();
    for route in &routes {
        if let Err(err) = add_route(luid_val, route) {
            let family_present = if route.addr().is_ipv4() { has_v4 } else { has_v6 };
            if family_present {
                return Err(os_err(&format!("ajout de la route {route}"), err));
            }
            log::warn!("route {route} ignorée (aucune adresse de cette famille sur l'interface)");
        }
    }

    if !iface.dns_servers.is_empty() || !iface.dns_search.is_empty() {
        let guid = interface_guid(luid_val)?;
        let v4: Vec<IpAddr> = iface.dns_servers.iter().copied().filter(IpAddr::is_ipv4).collect();
        let v6: Vec<IpAddr> = iface.dns_servers.iter().copied().filter(IpAddr::is_ipv6).collect();
        set_dns(guid, false, &v4, Some(&iface.dns_search))
            .map_err(|e| os_err("configuration DNS IPv4", e))?;
        if !v6.is_empty() {
            if let Err(e) = set_dns(guid, true, &v6, None) {
                if has_v6 {
                    bail!(os_err("configuration DNS IPv6", e));
                }
                log::warn!("DNS IPv6 ignorés : {}", std::io::Error::from_raw_os_error(e as i32));
            }
        }
    }
    Ok(())
}

/// Efface les réglages DNS persistés dans le registre pour cette interface.
pub fn clear_dns(luid_val: u64) {
    if let Ok(guid) = interface_guid(luid_val) {
        let _ = set_dns(guid, false, &[], Some(&[]));
        let _ = set_dns(guid, true, &[], None);
    }
}

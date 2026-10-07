//! Règles nftables du tunnel complet sous Linux, reprises de `wg-quick`.
//!
//! En tunnel complet, les paquets WireGuard sortants portent une marque (fwmark)
//! qui les fait échapper à la table de routage du tunnel. Les réponses entrantes
//! n'ont pas cette marque : sans la recopier via conntrack, la poignée de main
//! n'aboutit pas. On ajoute aussi la protection anti-usurpation de `wg-quick`
//! (paquets adressés à l'IP du tunnel mais arrivés par une autre interface).
//!
//! Toutes les règles vivent dans la table `inet cryptonaute`, supprimée à la
//! déconnexion.

use std::io::Write;
use std::net::IpAddr;
use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use ipnet::IpNet;

const TABLE: &str = "inet cryptonaute";

fn nft(script: &str) -> anyhow::Result<()> {
    let mut child = Command::new("nft")
        .args(["-f", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("la commande nft (paquet nftables) est requise pour le tunnel complet")?;
    child
        .stdin
        .take()
        .expect("stdin redirigé")
        .write_all(script.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("nft : {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

pub fn apply(ifname: &str, fwmark: u32, addresses: &[IpNet]) -> anyhow::Result<()> {
    let mut raw = String::new();
    for addr in addresses {
        let family = match addr.addr() {
            IpAddr::V4(_) => "ip",
            IpAddr::V6(_) => "ip6",
        };
        raw.push_str(&format!(
            "    iifname != \"{ifname}\" {family} daddr {} fib saddr type != local drop\n",
            addr.addr()
        ));
    }
    let script = format!(
        "table {TABLE} {{}}\n\
         delete table {TABLE}\n\
         table {TABLE} {{\n\
         \x20 chain preraw {{\n\
         \x20   type filter hook prerouting priority raw; policy accept;\n\
         {raw}\
         \x20 }}\n\
         \x20 chain premangle {{\n\
         \x20   type filter hook prerouting priority mangle; policy accept;\n\
         \x20   meta l4proto udp meta mark set ct mark\n\
         \x20 }}\n\
         \x20 chain postmangle {{\n\
         \x20   type filter hook postrouting priority mangle; policy accept;\n\
         \x20   meta l4proto udp meta mark {fwmark:#x} ct mark set meta mark\n\
         \x20 }}\n\
         }}\n"
    );
    nft(&script)
}

/// Choisit la marque (et table de routage) du tunnel complet comme `wg-quick` :
/// la première table libre à partir de 51820.
pub fn free_fwmark() -> u32 {
    let in_use = |family: &str, table: u32| {
        Command::new("ip")
            .args([family, "route", "show", "table", &table.to_string()])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    };
    (51820..52820)
        .find(|&t| !in_use("-4", t) && !in_use("-6", t))
        .unwrap_or(51820)
}

/// Supprime les règles (sans erreur si elles n'existent pas).
pub fn remove() {
    let _ = nft(&format!("table {TABLE} {{}}\ndelete table {TABLE}\n"));
}

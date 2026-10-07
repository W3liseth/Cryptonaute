//! Installation du démon : unité systemd (Linux) ou LaunchDaemon (macOS).
//! Nécessite root ; utilisé par les scripts des paquets .deb/.rpm/.pkg.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};

fn require_root() -> anyhow::Result<()> {
    // SAFETY: appel système sans effet de bord.
    if unsafe { libc::geteuid() } != 0 {
        bail!("cette commande doit être exécutée en root (sudo)");
    }
    Ok(())
}

/// Le binaire va s'exécuter en root : il ne doit être modifiable que par root,
/// sinon n'importe quel utilisateur pourrait le remplacer.
fn trusted_exe() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let mut path: &Path = &exe;
    loop {
        let meta = fs::metadata(path)?;
        if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
            bail!(
                "{} doit appartenir à root et ne pas être modifiable par les autres utilisateurs",
                path.display()
            );
        }
        match path.parent() {
            Some(parent) => path = parent,
            None => break,
        }
    }
    Ok(exe)
}

fn run(cmd: &str, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("exécution de {cmd}"))?;
    if !status.success() {
        bail!("{cmd} {} a échoué ({status})", args.join(" "));
    }
    Ok(())
}

/// Retire le dossier (vide) du socket laissé par le démon arrêté.
fn remove_socket_dir() {
    if let Some(dir) = Path::new(cryptonaute_common::ipc::SOCKET_PATH).parent() {
        let _ = fs::remove_dir(dir);
    }
}

fn write_root_file(path: &str, content: &str) -> anyhow::Result<()> {
    fs::write(path, content).with_context(|| format!("écriture de {path}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))?;
    Ok(())
}

#[cfg(target_os = "linux")]
const UNIT_PATH: &str = "/etc/systemd/system/cryptonaute.service";

#[cfg(target_os = "linux")]
pub fn install() -> anyhow::Result<()> {
    require_root()?;
    let exe = trusted_exe()?;
    let unit = format!(
        "[Unit]\n\
         Description=Cryptonaute VPN (service WireGuard)\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} run\n\
         Restart=on-failure\n\
         RestartSec=3\n\
         NoNewPrivileges=yes\n\
         ProtectHome=yes\n\
         PrivateTmp=yes\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        exe.display()
    );
    write_root_file(UNIT_PATH, &unit)?;
    run("systemctl", &["daemon-reload"])?;
    run("systemctl", &["enable", "cryptonaute.service"])?;
    // `restart` démarre le démon, ou le relance avec le nouveau binaire lors d'une mise à jour.
    run("systemctl", &["restart", "cryptonaute.service"])?;
    println!("Service cryptonaute installé et démarré.");
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn uninstall() -> anyhow::Result<()> {
    require_root()?;
    let _ = run("systemctl", &["disable", "--now", "cryptonaute.service"]);
    if Path::new(UNIT_PATH).exists() {
        fs::remove_file(UNIT_PATH)?;
    }
    let _ = run("systemctl", &["daemon-reload"]);
    remove_socket_dir();
    println!("Service cryptonaute supprimé.");
    Ok(())
}

#[cfg(target_os = "macos")]
const LABEL: &str = "fr.cryptonaute.service";
#[cfg(target_os = "macos")]
const PLIST_PATH: &str = "/Library/LaunchDaemons/fr.cryptonaute.service.plist";

#[cfg(target_os = "macos")]
pub fn install() -> anyhow::Result<()> {
    require_root()?;
    let exe = trusted_exe()?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array><string>{}</string><string>run</string></array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>StandardErrorPath</key><string>/var/log/cryptonaute.log</string>
</dict>
</plist>
"#,
        exe.display()
    );
    // Réinstallation : on décharge l'éventuelle version précédente.
    let _ = run("launchctl", &["bootout", &format!("system/{LABEL}")]);
    write_root_file(PLIST_PATH, &plist)?;
    run("launchctl", &["bootstrap", "system", PLIST_PATH])?;
    println!("Service {LABEL} installé et démarré.");
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> anyhow::Result<()> {
    require_root()?;
    let _ = run("launchctl", &["bootout", &format!("system/{LABEL}")]);
    if Path::new(PLIST_PATH).exists() {
        fs::remove_file(PLIST_PATH)?;
    }
    remove_socket_dir();
    println!("Service {LABEL} supprimé.");
    Ok(())
}

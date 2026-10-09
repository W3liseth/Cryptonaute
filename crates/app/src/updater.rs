//! Mises à jour de Cryptonaute depuis les releases GitHub.
//!
//! Le plugin de mise à jour de Tauri ne remplace que l'application (`.app`,
//! AppImage) : le service privilégié resterait dans son ancienne version. On
//! réutilise donc les installeurs complets de la release (`.exe`, `.deb`, `.rpm`,
//! `.pkg`), qui mettent à jour l'application et le service ensemble :
//! 1. la dernière release est lue via l'API GitHub ;
//! 2. l'installeur de la plateforme est téléchargé, puis sa signature minisign
//!    (fichier `.sig` produit par la CI) est vérifiée avec la clé publique
//!    intégrée à l'application, nom du fichier compris (pas de retour en arrière) ;
//! 3. l'installeur est lancé : l'utilisateur confirme l'élévation (UAC,
//!    polkit ou Installer.app), comme pour une installation manuelle.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};

const REPO: &str = "W3liseth/Cryptonaute";
/// Clé publique minisign (format Tauri : fichier de clé encodé en base64).
const PUBLIC_KEY: &str = include_str!("../updater.pub");
/// Taille maximale acceptée pour un installeur.
const MAX_INSTALLER_SIZE: u64 = 200 * 1024 * 1024;

/// Mise à jour disponible, telle que présentée à l'interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// Notes de version (Markdown de la release, affiché en texte brut).
    pub notes: String,
    /// Page de la release.
    pub page: String,
    /// Installeur de cette plateforme ; absent s'il n'y en a pas (ex. Mac Intel).
    pub asset: Option<String>,
    pub size: u64,
    #[serde(skip)]
    url: String,
    #[serde(skip)]
    sig_url: String,
}

/// Dernière mise à jour trouvée, partagée entre l'interface et la zone de notification.
#[derive(Default)]
pub struct UpdateState {
    pub available: Mutex<Option<UpdateInfo>>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(timeout))
        .user_agent(concat!("Cryptonaute/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn net_err(e: ureq::Error) -> String {
    format!("impossible de joindre GitHub : {e}")
}

/// Format des paquets installés (Linux) : déterminé par le gestionnaire qui
/// connaît Cryptonaute.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq)]
enum Package {
    Deb,
    Rpm,
}

#[cfg(target_os = "linux")]
fn installed_package() -> Option<Package> {
    use std::process::{Command, Stdio};
    let ok = |cmd: &str, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    if ok("dpkg-query", &["-W", "cryptonaute"]) {
        Some(Package::Deb)
    } else if ok("rpm", &["-q", "Cryptonaute"]) {
        Some(Package::Rpm)
    } else {
        None
    }
}

/// Vrai si `name` est l'installeur de cette plateforme.
fn is_platform_asset(name: &str) -> bool {
    #[cfg(windows)]
    return cfg!(target_arch = "x86_64") && name.ends_with("_x64-setup.exe");
    #[cfg(target_os = "macos")]
    return name.ends_with(if cfg!(target_arch = "aarch64") { "_arm64.pkg" } else { "_x86_64.pkg" });
    #[cfg(target_os = "linux")]
    return cfg!(target_arch = "x86_64")
        && match installed_package() {
            Some(Package::Deb) => name.ends_with("_amd64.deb"),
            Some(Package::Rpm) => name.ends_with(".x86_64.rpm"),
            None => false,
        };
}

/// Interroge GitHub : renvoie la mise à jour si une version plus récente existe.
pub fn check() -> Result<Option<UpdateInfo>, String> {
    check_from(env!("CARGO_PKG_VERSION"))
}

fn check_from(current: &str) -> Result<Option<UpdateInfo>, String> {
    let current = semver::Version::parse(current).map_err(|e| e.to_string())?;
    let release: Release = agent(Duration::from_secs(20))
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(net_err)?
        .body_mut()
        .read_json()
        .map_err(|e| format!("réponse de GitHub illisible : {e}"))?;

    let tag = release.tag_name.trim_start_matches('v');
    let latest = semver::Version::parse(tag).map_err(|_| format!("version de release invalide : {tag}"))?;
    if latest <= current {
        return Ok(None);
    }
    // Un installeur n'est proposé que s'il est accompagné de sa signature.
    let installer = release.assets.iter().find(|a| {
        is_platform_asset(&a.name) && release.assets.iter().any(|s| s.name == format!("{}.sig", a.name))
    });
    let sig_url = installer
        .and_then(|a| release.assets.iter().find(|s| s.name == format!("{}.sig", a.name)))
        .map(|s| s.browser_download_url.clone())
        .unwrap_or_default();
    Ok(Some(UpdateInfo {
        version: latest.to_string(),
        current: current.to_string(),
        notes: release.body.unwrap_or_default().trim().to_string(),
        page: release.html_url,
        asset: installer.map(|a| a.name.clone()),
        size: installer.map_or(0, |a| a.size),
        url: installer.map(|a| a.browser_download_url.clone()).unwrap_or_default(),
        sig_url,
    }))
}

/// Vérifie la signature minisign de `data`, émise pour le fichier `name`.
fn verify(data: &[u8], sig_b64: &str, name: &str) -> Result<(), String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let decode = |s: &str| {
        b64.decode(s.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| "encodage invalide".to_string())
    };
    let key = minisign_verify::PublicKey::decode(&decode(PUBLIC_KEY)?).map_err(|e| format!("clé publique : {e}"))?;
    let sig = minisign_verify::Signature::decode(&decode(sig_b64)?).map_err(|e| format!("signature : {e}"))?;
    key.verify(data, &sig, false)
        .map_err(|_| "la signature de l'installeur est invalide : mise à jour refusée".to_string())?;
    // Le commentaire signé contient le nom du fichier : un ancien installeur signé
    // ne peut pas être présenté sous le nom d'une version plus récente.
    if !sig.trusted_comment().split('\t').any(|f| f == format!("file:{name}")) {
        return Err("la signature ne correspond pas à cet installeur : mise à jour refusée".into());
    }
    Ok(())
}

/// Télécharge l'installeur et vérifie sa signature. `progress(reçu, total)`.
pub fn download(info: &UpdateInfo, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf, String> {
    let name = info.asset.as_deref().ok_or("aucun installeur pour cette plateforme")?;
    // Le nom vient de GitHub : il ne doit désigner qu'un fichier du dossier temporaire.
    if name.contains(['/', '\\']) || name.starts_with('.') {
        return Err("nom d'installeur invalide".into());
    }
    let http = agent(Duration::from_secs(600));
    let sig = http
        .get(&info.sig_url)
        .call()
        .map_err(net_err)?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;

    let mut resp = http.get(&info.url).call().map_err(net_err)?;
    let total = resp.body().content_length().unwrap_or(info.size);
    if total > MAX_INSTALLER_SIZE {
        return Err("installeur anormalement volumineux".into());
    }
    let mut reader = resp.body_mut().with_config().limit(MAX_INSTALLER_SIZE).reader();
    let mut data = Vec::with_capacity(total as usize);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("téléchargement interrompu : {e}"))?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        progress(data.len() as u64, total);
    }
    verify(&data, &sig, name)?;

    let dir = std::env::temp_dir().join("cryptonaute-update");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    let mut file = File::create(&path).map_err(|e| format!("écriture de l'installeur : {e}"))?;
    file.write_all(&data).map_err(|e| format!("écriture de l'installeur : {e}"))?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(path)
}

/// Comportement de l'application une fois l'installeur lancé.
pub enum AfterLaunch {
    /// Quitter : l'installeur remplace les fichiers (et relance l'application sous Windows).
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Exit,
    /// L'installation est terminée : relancer l'application (Linux).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Restart,
}

/// Windows : installeur NSIS en mode passif (`/P`), mise à jour (`/UPDATE`) et
/// relance de l'application à la fin (`/R`). `ShellExecuteEx` déclenche l'invite
/// UAC et ne rend la main qu'une fois l'élévation acceptée ou refusée.
#[cfg(windows)]
pub fn launch(path: &Path) -> Result<AfterLaunch, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain([0]).collect::<Vec<u16>>();
    let file = wide(path.as_os_str());
    let params = wide("/P /UPDATE /R".as_ref());
    // SAFETY: structure initialisée à zéro puis complétée ; les tampons UTF-16
    // restent vivants pendant l'appel ; le handle de processus est refermé.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_SHOWNORMAL;
        if ShellExecuteExW(&mut info) == 0 {
            return Err(if GetLastError() == ERROR_CANCELLED {
                "mise à jour annulée : l'autorisation administrateur a été refusée".into()
            } else {
                format!("lancement de l'installeur impossible : {}", std::io::Error::last_os_error())
            });
        }
        if !info.hProcess.is_null() {
            CloseHandle(info.hProcess);
        }
    }
    Ok(AfterLaunch::Exit)
}

/// macOS : le paquet est ouvert dans Installer.app, qui demande l'autorisation
/// administrateur et met à jour l'application et le démon.
#[cfg(target_os = "macos")]
pub fn launch(path: &Path) -> Result<AfterLaunch, String> {
    let status = std::process::Command::new("open")
        .arg(path)
        .status()
        .map_err(|e| format!("ouverture de l'installeur impossible : {e}"))?;
    if !status.success() {
        return Err("ouverture de l'installeur impossible".into());
    }
    Ok(AfterLaunch::Exit)
}

/// Linux : installation du paquet par le gestionnaire du système, via pkexec
/// (fenêtre d'authentification polkit). Bloquant jusqu'à la fin de l'installation.
#[cfg(target_os = "linux")]
pub fn launch(path: &Path) -> Result<AfterLaunch, String> {
    use std::process::Command;
    let exists = |p: &str| Path::new(p).exists();
    let file = path.to_string_lossy().to_string();
    let args: Vec<&str> = match installed_package() {
        Some(Package::Deb) => vec!["apt-get", "install", "-y", &file],
        Some(Package::Rpm) if exists("/usr/bin/dnf") => vec!["dnf", "install", "-y", &file],
        Some(Package::Rpm) if exists("/usr/bin/zypper") => {
            vec!["zypper", "--non-interactive", "install", "--allow-unsigned-rpm", &file]
        }
        Some(Package::Rpm) => vec!["rpm", "-U", &file],
        None => return Err("paquet Cryptonaute introuvable : mettez à jour manuellement".into()),
    };
    let manual = format!("sudo {}", args.join(" "));
    let status = Command::new("pkexec")
        .args(&args)
        .status()
        .map_err(|_| format!("pkexec est introuvable : installez la mise à jour avec « {manual} »"))?;
    match status.code() {
        Some(0) => Ok(AfterLaunch::Restart),
        // 126 : authentification refusée ou fenêtre fermée.
        Some(126) | Some(127) => Err("mise à jour annulée : l'autorisation administrateur a été refusée".into()),
        _ => Err(format!("l'installation a échoué ({status}) ; vous pouvez la relancer avec « {manual} »")),
    }
}

/// Ouvre la page d'une release dans le navigateur. Seules les pages du dépôt
/// sont acceptées : l'adresse vient de l'API GitHub.
pub fn open_page(url: &str) -> Result<(), String> {
    let prefix = format!("https://github.com/{REPO}/");
    if !url.starts_with(&prefix) || url.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err("adresse de page inattendue".into());
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (verb, target) = (wide("open"), wide(url));
        // SAFETY: chaînes UTF-16 terminées par un zéro, vivantes pendant l'appel.
        let rc = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // Une valeur supérieure à 32 indique un succès.
        if rc as isize <= 32 {
            return Err("ouverture du navigateur impossible".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        std::process::Command::new(opener)
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("ouverture du navigateur impossible : {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La clé publique intégrée doit être lisible.
    #[test]
    fn embedded_public_key_decodes() {
        let b64 = base64::engine::general_purpose::STANDARD;
        let text = String::from_utf8(b64.decode(PUBLIC_KEY.trim()).unwrap()).unwrap();
        minisign_verify::PublicKey::decode(&text).unwrap();
    }

    /// Signature réelle de `TEST_DATA` produite par `tauri signer sign` avec la
    /// clé privée de Cryptonaute, pour un fichier `Cryptonaute_9.9.9_x64-setup.exe`.
    const TEST_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSZ0xGYUVFNU9iNVZWNnBaM2E2SzFRRFl1TS9iOFBNK3B5VjhCUlRwVERDTC9QVnNvbGl3b1pMTDcxL3V5SDBNOTE5dmd3cS8vaTlQcUdCYk1GUDd4di85VVpnRnIwV0FVPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxNTM2NjQwCWZpbGU6Q3J5cHRvbmF1dGVfOS45LjlfeDY0LXNldHVwLmV4ZQpRWk9ReW9pUHpSbkVyMUJnSkY5eEpkVHBlK0NYYy9BaDUwNGNSR2VrU3Y1eEs5Vmo0MXdvMG5hVmFWdG52TkxXSnNiUnYrUWtPU0p3cFhHOXFYb1hBZz09Cg==";
    const TEST_NAME: &str = "Cryptonaute_9.9.9_x64-setup.exe";

    fn test_data() -> Vec<u8> {
        (1..=200).collect()
    }

    /// Interroge la vraie release GitHub (réseau requis).
    #[test]
    #[ignore = "nécessite un accès à GitHub"]
    fn finds_newer_release() {
        let info = check_from("0.1.0").unwrap().expect("une release plus récente que 0.1.0");
        println!("{} -> {} : {:?} ({} octets)", info.current, info.version, info.asset, info.size);
        assert!(check_from("999.0.0").unwrap().is_none());
    }

    #[test]
    fn accepts_genuine_signature() {
        verify(&test_data(), TEST_SIG, TEST_NAME).unwrap();
    }

    #[test]
    fn rejects_tampered_or_renamed_installer() {
        let mut tampered = test_data();
        tampered[0] ^= 1;
        assert!(verify(&tampered, TEST_SIG, TEST_NAME).is_err());
        // Ancien installeur signé présenté comme une autre version.
        assert!(verify(&test_data(), TEST_SIG, "Cryptonaute_9.9.10_x64-setup.exe").is_err());
        assert!(verify(&test_data(), "", TEST_NAME).is_err());
    }
}

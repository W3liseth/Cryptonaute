//! Lancement automatique à l'ouverture de session, propre à chaque utilisateur
//! et sans droit administrateur :
//! - Windows : valeur `Cryptonaute` de `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` ;
//! - Linux : `~/.config/autostart/cryptonaute.desktop` (spécification XDG Autostart) ;
//! - macOS : `~/Library/LaunchAgents/fr.cryptonaute.client.plist`.
//!
//! L'application est démarrée avec `--minimized`, directement dans la zone de
//! notification lorsque l'icône est activée.

pub use platform::{is_enabled, set_enabled};

/// Chemin de l'exécutable courant.
fn current_exe() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.display().to_string())
}

/// Si l'option est active mais pointe vers un autre emplacement (application
/// déplacée ou réinstallée ailleurs), met à jour l'entrée enregistrée.
pub fn repair() {
    migrate_legacy();
    if let (Ok(Some(entry)), Ok(expected)) = (platform::current_entry(), platform::expected_entry()) {
        if entry != expected {
            let _ = set_enabled(true);
        }
    }
}

/// Cryptonaute s'appelait « RustGuard » jusqu'à la version 0.2.1 : si l'ancienne
/// entrée de démarrage existe, elle est remplacée par la nouvelle.
fn migrate_legacy() {
    if platform::remove_legacy() {
        let _ = set_enabled(true);
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, NO_ERROR};
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "Cryptonaute";
    const LEGACY_VALUE_NAME: &str = "RustGuard";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn os_err(code: u32) -> String {
        format!("registre : {}", std::io::Error::from_raw_os_error(code as i32))
    }

    pub fn expected_entry() -> Result<String, String> {
        Ok(format!("\"{}\" --minimized", super::current_exe()?))
    }

    /// Valeur actuellement enregistrée, si elle existe.
    pub fn current_entry() -> Result<Option<String>, String> {
        let (key, name) = (wide(RUN_KEY), wide(VALUE_NAME));
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: tampon et taille cohérents ; chaînes terminées par un zéro.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        match err {
            NO_ERROR => {
                let len = (size as usize / 2).saturating_sub(1);
                Ok(Some(String::from_utf16_lossy(&buf[..len])))
            }
            ERROR_FILE_NOT_FOUND => Ok(None),
            e => Err(os_err(e)),
        }
    }

    pub fn is_enabled() -> bool {
        matches!(current_entry(), Ok(Some(_)))
    }

    /// Supprime l'entrée de l'ancien nom ; vrai si elle existait.
    pub fn remove_legacy() -> bool {
        let (key, name) = (wide(RUN_KEY), wide(LEGACY_VALUE_NAME));
        // SAFETY: chaînes terminées par un zéro.
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) == NO_ERROR }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let (key, name) = (wide(RUN_KEY), wide(VALUE_NAME));
        let err = if enabled {
            let data = wide(&expected_entry()?);
            // SAFETY: `data` est une chaîne UTF-16 terminée par un zéro de `len * 2` octets.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            }
        } else {
            // SAFETY: chaînes terminées par un zéro.
            match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) } {
                ERROR_FILE_NOT_FOUND => NO_ERROR,
                e => e,
            }
        };
        if err == NO_ERROR {
            Ok(())
        } else {
            Err(os_err(err))
        }
    }
}

#[cfg(unix)]
mod platform {
    use std::fs;
    use std::path::PathBuf;

    use crate::paths::home_dir;

    #[cfg(target_os = "macos")]
    fn entry_path() -> Result<PathBuf, String> {
        Ok(home_dir()?.join("Library/LaunchAgents/fr.cryptonaute.client.plist"))
    }

    #[cfg(target_os = "macos")]
    fn legacy_entry_path() -> Result<PathBuf, String> {
        Ok(home_dir()?.join("Library/LaunchAgents/fr.rustguard.client.plist"))
    }

    #[cfg(target_os = "macos")]
    pub fn expected_entry() -> Result<String, String> {
        let exe = xml_escape(&super::current_exe()?);
        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>fr.cryptonaute.client</string>
    <key>ProgramArguments</key>
    <array><string>{exe}</string><string>--minimized</string></array>
    <key>RunAtLoad</key><true/>
    <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
        ))
    }

    #[cfg(target_os = "macos")]
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    #[cfg(not(target_os = "macos"))]
    fn autostart_dir() -> Result<PathBuf, String> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .map_or_else(|| home_dir().map(|h| h.join(".config")), Ok)?;
        Ok(config.join("autostart"))
    }

    #[cfg(not(target_os = "macos"))]
    fn entry_path() -> Result<PathBuf, String> {
        Ok(autostart_dir()?.join("cryptonaute.desktop"))
    }

    #[cfg(not(target_os = "macos"))]
    fn legacy_entry_path() -> Result<PathBuf, String> {
        Ok(autostart_dir()?.join("rustguard.desktop"))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn expected_entry() -> Result<String, String> {
        // Spécification Desktop Entry : chemin entre guillemets, `"`, `` ` ``, `$` et `\` échappés.
        let exe: String = super::current_exe()?
            .chars()
            .flat_map(|c| match c {
                '"' | '`' | '$' | '\\' => vec!['\\', c],
                c => vec![c],
            })
            .collect();
        Ok(format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Cryptonaute\n\
             Comment=Client WireGuard\n\
             Exec=\"{exe}\" --minimized\n\
             Icon=cryptonaute\n\
             Terminal=false\n\
             X-GNOME-Autostart-enabled=true\n"
        ))
    }

    pub fn current_entry() -> Result<Option<String>, String> {
        match fs::read_to_string(entry_path()?) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn is_enabled() -> bool {
        matches!(current_entry(), Ok(Some(_)))
    }

    /// Supprime l'entrée de l'ancien nom ; vrai si elle existait.
    pub fn remove_legacy() -> bool {
        legacy_entry_path().is_ok_and(|p| fs::remove_file(p).is_ok())
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let path = entry_path()?;
        if enabled {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            fs::write(&path, expected_entry()?).map_err(|e| format!("écriture de {} : {e}", path.display()))
        } else {
            match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("suppression de {} : {e}", path.display())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Modifie temporairement la configuration de démarrage de l'utilisateur, puis la restaure.
    #[test]
    #[ignore = "modifie la configuration de démarrage de l'utilisateur courant"]
    fn toggle_and_restore() {
        let before = platform::current_entry().unwrap();
        set_enabled(true).unwrap();
        assert!(is_enabled());
        assert_eq!(platform::current_entry().unwrap(), Some(platform::expected_entry().unwrap()));
        assert!(platform::expected_entry().unwrap().contains("--minimized"));
        set_enabled(false).unwrap();
        assert!(!is_enabled());
        set_enabled(false).unwrap(); // idempotent
        if before.is_some() {
            set_enabled(true).unwrap();
        }
    }
}

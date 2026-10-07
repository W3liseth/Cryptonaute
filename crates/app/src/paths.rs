//! Dossier de données de l'utilisateur, selon la plateforme :
//! - Windows : `%LOCALAPPDATA%\Cryptonaute`
//! - macOS : `~/Library/Application Support/Cryptonaute`
//! - Linux : `$XDG_DATA_HOME/cryptonaute` (par défaut `~/.local/share/cryptonaute`)

use std::path::PathBuf;

#[cfg(windows)]
pub fn data_dir() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("Cryptonaute"))
        .ok_or_else(|| "variable LOCALAPPDATA introuvable".into())
}

#[cfg(unix)]
pub fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "variable HOME introuvable".into())
}

#[cfg(target_os = "macos")]
pub fn data_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join("Library/Application Support/Cryptonaute"))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn data_dir() -> Result<PathBuf, String> {
    match std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        Some(d) => Ok(PathBuf::from(d).join("cryptonaute")),
        None => Ok(home_dir()?.join(".local/share/cryptonaute")),
    }
}

/// Dossier de données de RustGuard, l'ancien nom de Cryptonaute (≤ 0.2.1).
#[cfg(windows)]
fn legacy_data_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("RustGuard"))
}

#[cfg(target_os = "macos")]
fn legacy_data_dir() -> Option<PathBuf> {
    home_dir().ok().map(|h| h.join("Library/Application Support/RustGuard"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn legacy_data_dir() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        Some(d) => Some(PathBuf::from(d).join("rustguard")),
        None => home_dir().ok().map(|h| h.join(".local/share/rustguard")),
    }
}

/// Reprend les tunnels et préférences enregistrés sous l'ancien nom. Si le
/// nouveau dossier n'existe pas encore, l'ancien est simplement renommé ;
/// sinon, les tunnels absents du nouveau dossier y sont déplacés.
pub fn migrate_legacy_data() {
    if let (Some(old), Ok(new)) = (legacy_data_dir(), data_dir()) {
        migrate_dir(&old, &new);
    }
}

fn migrate_dir(old: &std::path::Path, new: &std::path::Path) {
    use std::fs;
    if !old.is_dir() {
        return;
    }
    if !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::rename(old, new).is_ok() {
            return;
        }
    }
    let (old_tunnels, new_tunnels) = (old.join("tunnels"), new.join("tunnels"));
    let _ = fs::create_dir_all(&new_tunnels);
    for entry in fs::read_dir(&old_tunnels).into_iter().flatten().flatten() {
        let target = new_tunnels.join(entry.file_name());
        if !target.exists() {
            let _ = fs::rename(entry.path(), target);
        }
    }
    if !new.join("settings.json").exists() {
        let _ = fs::rename(old.join("settings.json"), new.join("settings.json"));
    }
    // Ne supprime que des dossiers devenus vides.
    let _ = fs::remove_dir(&old_tunnels);
    let _ = fs::remove_dir(old);
}

#[cfg(test)]
mod tests {
    use super::migrate_dir;
    use std::fs;

    #[test]
    fn migrates_legacy_data() {
        let root = std::env::temp_dir().join(format!("cryptonaute-migr-{}", std::process::id()));
        let (old, new) = (root.join("old"), root.join("new"));
        let _ = fs::remove_dir_all(&root);

        // Cas 1 : le nouveau dossier n'existe pas, l'ancien est renommé.
        fs::create_dir_all(old.join("tunnels")).unwrap();
        fs::write(old.join("tunnels/a.conf"), "a").unwrap();
        fs::write(old.join("settings.json"), "{}").unwrap();
        migrate_dir(&old, &new);
        assert!(!old.exists());
        assert_eq!(fs::read_to_string(new.join("tunnels/a.conf")).unwrap(), "a");
        assert!(new.join("settings.json").exists());

        // Cas 2 : les deux existent, seuls les tunnels absents sont déplacés.
        fs::create_dir_all(old.join("tunnels")).unwrap();
        fs::write(old.join("tunnels/a.conf"), "ancien").unwrap();
        fs::write(old.join("tunnels/b.conf"), "b").unwrap();
        migrate_dir(&old, &new);
        assert_eq!(fs::read_to_string(new.join("tunnels/a.conf")).unwrap(), "a", "pas d'écrasement");
        assert_eq!(fs::read_to_string(new.join("tunnels/b.conf")).unwrap(), "b");
        assert!(old.join("tunnels/a.conf").exists(), "le conflit reste en place");

        let _ = fs::remove_dir_all(&root);
    }
}

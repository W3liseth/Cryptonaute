//! Stockage des configurations dans le profil de l'utilisateur. Aucun droit
//! administrateur n'est nécessaire pour ajouter, modifier ou supprimer un tunnel.
//!
//! - Windows : `%LOCALAPPDATA%\Cryptonaute\tunnels`, fichiers chiffrés avec DPAPI :
//!   seul le compte Windows qui les a enregistrés peut les déchiffrer.
//! - Linux / macOS : fichiers lisibles par leur seul propriétaire (0600, dossier
//!   0700), comme `/etc/wireguard` pour `wg-quick`.

use std::fs;
use std::path::PathBuf;

use cryptonaute_common::config::{validate_tunnel_name, MAX_CONFIG_LEN};

use crate::paths;

pub struct Store {
    dir: PathBuf,
}

#[cfg(windows)]
mod protect {
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    pub const EXTENSION: &str = "conf.dpapi";
    const ENTROPY: &[u8] = b"Cryptonaute tunnel configuration v1";
    /// Entropie des configurations enregistrées par RustGuard (ancien nom, ≤ 0.2.1).
    const LEGACY_ENTROPY: &[u8] = b"RustGuard tunnel configuration v1";

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    fn dpapi(data: &[u8], protect: bool, entropy: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(data);
        let entropy = blob(entropy);
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: les blobs d'entrée pointent vers des tampons valides pendant l'appel ;
        // la sortie est allouée par le système et libérée avec LocalFree.
        let ok = unsafe {
            if protect {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    &entropy,
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    &entropy,
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if ok == 0 {
            return Err(format!("DPAPI : {}", std::io::Error::last_os_error()));
        }
        // SAFETY: `output` décrit une allocation valide de `cbData` octets.
        let out = unsafe {
            let v = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
            LocalFree(output.pbData as HLOCAL);
            v
        };
        Ok(out)
    }

    pub fn seal(plain: &[u8]) -> Result<Vec<u8>, String> {
        dpapi(plain, true, ENTROPY)
    }

    #[cfg(test)]
    pub fn seal_legacy(plain: &[u8]) -> Result<Vec<u8>, String> {
        dpapi(plain, true, LEGACY_ENTROPY)
    }

    /// Déchiffre ; le booléen indique un fichier de l'ancienne version, à rechiffrer.
    pub fn open(sealed: &[u8]) -> Result<(Vec<u8>, bool), String> {
        match dpapi(sealed, false, ENTROPY) {
            Ok(plain) => Ok((plain, false)),
            Err(e) => dpapi(sealed, false, LEGACY_ENTROPY).map(|p| (p, true)).map_err(|_| e),
        }
    }

    pub fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, data)
    }

    pub fn restrict_dir(_dir: &std::path::Path) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(unix)]
mod protect {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::Path;

    pub const EXTENSION: &str = "conf";

    pub fn seal(plain: &[u8]) -> Result<Vec<u8>, String> {
        Ok(plain.to_vec())
    }

    pub fn open(sealed: &[u8]) -> Result<(Vec<u8>, bool), String> {
        Ok((sealed.to_vec(), false))
    }

    /// Écrit un fichier lisible par son seul propriétaire.
    pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
        let _ = fs::remove_file(path);
        let mut f = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        f.write_all(data)?;
        f.sync_all()
    }

    pub fn restrict_dir(dir: &Path) -> std::io::Result<()> {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
    }
}

use protect::EXTENSION;

impl Store {
    pub fn open() -> Result<Self, String> {
        let base = paths::data_dir()?;
        fs::create_dir_all(&base).map_err(|e| format!("création de {} : {e}", base.display()))?;
        protect::restrict_dir(&base).map_err(|e| e.to_string())?;
        Self::at(base.join("tunnels"))
    }

    fn at(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(|e| format!("création de {} : {e}", dir.display()))?;
        protect::restrict_dir(&dir).map_err(|e| e.to_string())?;
        Ok(Store { dir })
    }

    fn path(&self, name: &str) -> Result<PathBuf, String> {
        validate_tunnel_name(name)?;
        Ok(self.dir.join(format!("{name}.{EXTENSION}")))
    }

    /// Noms des tunnels enregistrés, triés sans tenir compte de la casse.
    pub fn names(&self) -> Vec<String> {
        let suffix = format!(".{EXTENSION}");
        let mut names: Vec<String> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|f| f.strip_suffix(&suffix).map(str::to_string))
            .filter(|n| validate_tunnel_name(n).is_ok())
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    pub fn load(&self, name: &str) -> Result<String, String> {
        let path = self.path(name)?;
        let data = fs::read(&path).map_err(|e| format!("lecture de « {name} » : {e}"))?;
        let (plain, legacy) = protect::open(&data)?;
        let text = String::from_utf8(plain).map_err(|_| "configuration corrompue".to_string())?;
        if legacy {
            // Rechiffre au format actuel (sans conséquence en cas d'échec : relu au prochain accès).
            let _ = self.rewrite(name, &text);
        }
        Ok(text)
    }

    /// Réécrit un tunnel existant de façon atomique.
    fn rewrite(&self, name: &str, text: &str) -> Result<(), String> {
        let sealed = protect::seal(text.as_bytes())?;
        let tmp = self.dir.join(format!(".{name}.tmp"));
        protect::write_private(&tmp, &sealed).map_err(|e| e.to_string())?;
        fs::rename(&tmp, self.path(name)?).map_err(|e| e.to_string())
    }

    /// Enregistre `text` sous `name`. Si `previous` est fourni (renommage ou
    /// édition), l'ancien fichier est remplacé ; sinon `name` ne doit pas exister.
    pub fn save(&self, name: &str, text: &str, previous: Option<&str>) -> Result<(), String> {
        if text.len() > MAX_CONFIG_LEN {
            return Err("configuration trop volumineuse".into());
        }
        let path = self.path(name)?;
        let renaming = previous.is_some_and(|p| !p.eq_ignore_ascii_case(name));
        // Unicité insensible à la casse, y compris sur les systèmes de fichiers qui y sont sensibles.
        let taken = self.names().iter().any(|n| n.eq_ignore_ascii_case(name));
        if (previous.is_none() || renaming) && taken {
            return Err(format!("un tunnel nommé « {name} » existe déjà"));
        }
        let sealed = protect::seal(text.as_bytes())?;
        let tmp = self.dir.join(format!(".{name}.tmp"));
        protect::write_private(&tmp, &sealed).map_err(|e| format!("écriture : {e}"))?;
        if let Some(old) = previous {
            let old_path = self.path(old)?;
            if renaming {
                fs::rename(&tmp, &path).map_err(|e| format!("enregistrement : {e}"))?;
                let _ = fs::remove_file(old_path);
                return Ok(());
            }
            // Même nom avec une casse différente : on retire l'ancien fichier pour
            // que le nouveau nom soit conservé tel quel.
            if old != name {
                let _ = fs::remove_file(old_path);
            }
        }
        fs::rename(&tmp, &path).map_err(|e| format!("enregistrement : {e}"))
    }

    pub fn delete(&self, name: &str) -> Result<(), String> {
        fs::remove_file(self.path(name)?).map_err(|e| format!("suppression de « {name} » : {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_rename_and_conflicts() {
        let dir = std::env::temp_dir().join(format!("cryptonaute-test-{}", std::process::id()));
        let store = Store::at(dir.clone()).unwrap();
        let secret = "[Interface]\nPrivateKey = secret\n";

        store.save("wg0", secret, None).unwrap();
        let raw = fs::read(dir.join(format!("wg0.{EXTENSION}"))).unwrap();
        if cfg!(windows) {
            assert!(!raw.windows(6).any(|w| w == b"secret"), "le fichier doit être chiffré");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join("wg0.conf")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "le fichier doit être privé");
        }
        assert_eq!(store.load("wg0").unwrap(), secret);

        assert!(store.save("WG0", secret, None).is_err(), "collision insensible à la casse");
        store.save("wg0", "v2", Some("wg0")).unwrap();
        assert_eq!(store.load("wg0").unwrap(), "v2");

        store.save("bureau", "v3", Some("wg0")).unwrap();
        assert_eq!(store.names(), vec!["bureau".to_string()]);
        assert!(store.load("../evil").is_err());

        store.delete("bureau").unwrap();
        assert!(store.names().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    /// Une configuration chiffrée par RustGuard (ancien nom) est lue puis rechiffrée.
    #[cfg(windows)]
    #[test]
    fn reads_and_reseals_legacy_dpapi() {
        let dir = std::env::temp_dir().join(format!("cryptonaute-legacy-{}", std::process::id()));
        let store = Store::at(dir.clone()).unwrap();
        let legacy = protect::seal_legacy(b"ancienne configuration").unwrap();
        fs::write(dir.join(format!("vieux.{EXTENSION}")), &legacy).unwrap();

        assert_eq!(store.load("vieux").unwrap(), "ancienne configuration");
        let resealed = fs::read(dir.join(format!("vieux.{EXTENSION}"))).unwrap();
        assert_ne!(resealed, legacy, "le fichier doit être rechiffré");
        assert_eq!(protect::open(&resealed).unwrap(), (b"ancienne configuration".to_vec(), false));
        let _ = fs::remove_dir_all(dir);
    }
}

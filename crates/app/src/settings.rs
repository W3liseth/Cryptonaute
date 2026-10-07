//! Préférences de l'utilisateur, stockées dans `settings.json` du dossier de
//! données (voir `paths`).
//! (Le lancement au démarrage est géré par le système : voir `autostart`.)

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Afficher l'icône dans la zone de notification. Si elle est masquée,
    /// fermer la fenêtre quitte l'application.
    pub show_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { show_tray: true }
    }
}

fn path() -> Option<PathBuf> {
    crate::paths::data_dir().ok().map(|d| d.join("settings.json"))
}

pub fn load() -> Settings {
    path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or("dossier de données introuvable")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("enregistrement des options : {e}"))
}

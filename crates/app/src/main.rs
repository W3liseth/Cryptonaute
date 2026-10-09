//! Interface graphique de Cryptonaute (aucun droit administrateur requis).
//!
//! Les configurations sont gérées localement (stockage chiffré par utilisateur) ;
//! l'activation des tunnels est déléguée au service privilégié via IPC.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod client;
mod paths;
mod settings;
mod store;
mod tray;
mod updater;

use cryptonaute_common::config::{self, TunnelSummary, WgConfig};
use cryptonaute_common::ipc::{Request, Response, ServiceStatus};
use serde::Serialize;
use tauri::{Emitter, Manager};

use client::ClientError;
use store::Store;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TunnelEntry {
    name: String,
    summary: Option<TunnelSummary>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceView {
    available: bool,
    version: Option<String>,
    status: Option<ServiceStatus>,
    error: Option<String>,
}

/// Exécute une opération bloquante hors du thread de l'interface.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("tâche interrompue : {e}"))?
}

fn entry(store: &Store, name: String) -> TunnelEntry {
    match store.load(&name).and_then(|t| WgConfig::parse(&t).map_err(|e| e.to_string())) {
        Ok(cfg) => TunnelEntry {
            name,
            summary: Some(cfg.summary()),
            error: None,
        },
        Err(e) => TunnelEntry {
            name,
            summary: None,
            error: Some(e),
        },
    }
}

pub(crate) fn status_from(resp: Result<Response, ClientError>) -> Result<ServiceStatus, String> {
    match resp.map_err(|e| e.to_string())? {
        Response::Status(s) => Ok(s),
        Response::Error { message } => Err(message),
        Response::Hello { .. } => Err("réponse inattendue du service".into()),
    }
}

#[tauri::command]
async fn list_tunnels() -> Result<Vec<TunnelEntry>, String> {
    blocking(|| {
        let store = Store::open()?;
        Ok(store.names().into_iter().map(|n| entry(&store, n)).collect())
    })
    .await
}

#[tauri::command]
async fn get_tunnel_config(name: String) -> Result<String, String> {
    blocking(move || Store::open()?.load(&name)).await
}

#[tauri::command]
fn validate_config(config: String) -> Result<TunnelSummary, String> {
    WgConfig::parse(&config)
        .map(|c| c.summary())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn validate_name(name: String) -> Result<(), String> {
    config::validate_tunnel_name(&name)
}

#[tauri::command]
fn new_template() -> String {
    config::new_template()
}

#[tauri::command]
async fn save_tunnel(name: String, config: String, previous: Option<String>) -> Result<TunnelEntry, String> {
    blocking(move || {
        WgConfig::parse(&config).map_err(|e| e.to_string())?;
        let store = Store::open()?;
        store.save(&name, &config, previous.as_deref())?;
        Ok(entry(&store, name))
    })
    .await
}

#[tauri::command]
async fn delete_tunnel(name: String) -> Result<(), String> {
    blocking(move || Store::open()?.delete(&name)).await
}

#[tauri::command]
async fn service_status() -> Result<ServiceView, String> {
    blocking(|| {
        Ok(match status_from(client::request(&Request::Status)) {
            Ok(status) => ServiceView {
                available: true,
                version: None,
                status: Some(status),
                error: None,
            },
            Err(e) => ServiceView {
                available: false,
                version: None,
                status: None,
                error: Some(e),
            },
        })
    })
    .await
}

#[tauri::command]
async fn service_info() -> Result<ServiceView, String> {
    blocking(|| {
        Ok(match client::request(&Request::Hello) {
            Ok(Response::Hello { version, .. }) => ServiceView {
                available: true,
                version: Some(version),
                status: None,
                error: None,
            },
            Ok(_) => ServiceView {
                available: false,
                version: None,
                status: None,
                error: Some("réponse inattendue du service".into()),
            },
            Err(e) => ServiceView {
                available: false,
                version: None,
                status: None,
                error: Some(e.to_string()),
            },
        })
    })
    .await
}

/// Charge la configuration chiffrée du tunnel et demande son activation au service,
/// à côté des tunnels déjà actifs.
pub(crate) fn connect_tunnel(name: &str) -> Result<ServiceStatus, String> {
    let config = Store::open()?.load(name)?;
    status_from(client::request(&Request::Connect {
        name: name.to_string(),
        config,
    }))
}

/// Déconnecte le tunnel `name`, ou tous les tunnels.
pub(crate) fn disconnect_tunnel(name: Option<String>) -> Result<ServiceStatus, String> {
    status_from(client::request(&Request::Disconnect { name }))
}

#[tauri::command]
async fn connect(name: String) -> Result<ServiceStatus, String> {
    blocking(move || connect_tunnel(&name)).await
}

#[tauri::command]
async fn disconnect(name: Option<String>) -> Result<ServiceStatus, String> {
    blocking(move || disconnect_tunnel(name)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Options {
    show_tray: bool,
    autostart: bool,
    auto_update: bool,
}

fn current_options(app: &tauri::AppHandle) -> Options {
    Options {
        show_tray: tray::is_enabled(app),
        autostart: autostart::is_enabled(),
        auto_update: settings::load().auto_update,
    }
}

#[tauri::command]
fn get_options(app: tauri::AppHandle) -> Options {
    current_options(&app)
}

/// Modifie une ou plusieurs options ; les champs absents sont laissés inchangés.
#[tauri::command]
fn set_options(
    app: tauri::AppHandle,
    show_tray: Option<bool>,
    autostart: Option<bool>,
    auto_update: Option<bool>,
) -> Result<Options, String> {
    if let Some(visible) = show_tray {
        let mut prefs = settings::load();
        prefs.show_tray = visible;
        settings::save(&prefs)?;
        tray::set_visible(&app, visible).map_err(|e| e.to_string())?;
    }
    if let Some(enabled) = autostart {
        autostart::set_enabled(enabled)?;
    }
    if let Some(enabled) = auto_update {
        let mut prefs = settings::load();
        prefs.auto_update = enabled;
        settings::save(&prefs)?;
    }
    Ok(current_options(&app))
}

/// Délai avant la première recherche de mise à jour, puis intervalle entre deux recherches.
const UPDATE_FIRST_CHECK: std::time::Duration = std::time::Duration::from_secs(15);
const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

/// Mémorise le résultat d'une recherche et prévient l'interface et l'icône.
fn store_update(app: &tauri::AppHandle, found: Option<updater::UpdateInfo>) {
    let state = app.state::<updater::UpdateState>();
    let mut available = state.available.lock().unwrap_or_else(|e| e.into_inner());
    let is_new = found.as_ref().map(|u| &u.version) != available.as_ref().map(|u| &u.version);
    *available = found.clone();
    drop(available);
    if is_new {
        if let Some(info) = found {
            let _ = app.emit("update-available", info);
        }
        tray::refresh_now(app);
    }
}

/// Mise à jour déjà trouvée (sans nouvelle requête).
#[tauri::command]
fn update_status(app: tauri::AppHandle) -> Option<updater::UpdateInfo> {
    app.state::<updater::UpdateState>()
        .available
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<Option<updater::UpdateInfo>, String> {
    let found = blocking(updater::check).await?;
    store_update(&app, found.clone());
    Ok(found)
}

#[tauri::command]
fn open_release_page(app: tauri::AppHandle) -> Result<(), String> {
    let page = update_status(app)
        .map(|u| u.page)
        .unwrap_or_else(|| "https://github.com/W3liseth/Cryptonaute/releases/latest".into());
    updater::open_page(&page)
}

#[derive(Clone, Serialize)]
struct UpdateProgress {
    received: u64,
    total: u64,
}

/// Télécharge, vérifie et lance l'installeur de la mise à jour trouvée.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let info = update_status(app.clone()).ok_or("aucune mise à jour disponible")?;
    let handle = app.clone();
    let after = blocking(move || {
        let mut last = 0;
        let path = updater::download(&info, |received, total| {
            // Une notification tous les 256 Kio suffit à animer la barre de progression.
            if received - last >= 256 * 1024 || received == total {
                last = received;
                let _ = handle.emit("update-progress", UpdateProgress { received, total });
            }
        })?;
        updater::launch(&path)
    })
    .await?;
    match after {
        updater::AfterLaunch::Exit => app.exit(0),
        updater::AfterLaunch::Restart => app.restart(),
    }
    Ok(())
}

fn spawn_update_checks(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(UPDATE_FIRST_CHECK);
        loop {
            if settings::load().auto_update {
                match updater::check() {
                    Ok(found) => store_update(&app, found),
                    Err(e) => eprintln!("recherche de mise à jour : {e}"),
                }
            }
            std::thread::sleep(UPDATE_INTERVAL);
        }
    });
}

fn main() {
    let start_hidden = std::env::args().any(|a| a == "--minimized");
    tauri::Builder::default()
        // Un second lancement réaffiche simplement la fenêtre existante.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .setup(move |app| {
            // Reprise des données de RustGuard, l'ancien nom de l'application.
            paths::migrate_legacy_data();
            autostart::repair();
            let prefs = settings::load();
            app.manage(updater::UpdateState::default());
            tray::setup(app.handle(), prefs.show_tray)?;
            spawn_update_checks(app.handle().clone());
            // Sans icône de notification, une fenêtre masquée serait inaccessible.
            if !start_hidden || !prefs.show_tray {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        // Avec l'icône de notification, fermer la fenêtre la masque ; sinon l'application se ferme.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if tray::is_enabled(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_tunnels,
            get_tunnel_config,
            validate_config,
            validate_name,
            new_template,
            save_tunnel,
            delete_tunnel,
            service_status,
            service_info,
            connect,
            disconnect,
            get_options,
            set_options,
            update_status,
            check_update,
            install_update,
            open_release_page,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Cryptonaute");
}

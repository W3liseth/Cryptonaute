//! Icône de la zone de notification : état de connexion en un coup d'œil et
//! menu permettant de (dé)connecter les tunnels sans ouvrir la fenêtre (plusieurs
//! peuvent être actifs à la fois).
//! L'utilisateur peut la masquer depuis les options de l'application.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use cryptonaute_common::ipc::Request;
use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::updater::UpdateState;
use crate::{client, status_from, store::Store};

const TRAY_ID: &str = "main";
const ICON_ON: &[u8] = include_bytes!("../icons/tray-on.png");
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png");
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Ce qui est affiché par l'icône ; elle n'est mise à jour que s'il change.
#[derive(Clone, Default, PartialEq)]
struct Snapshot {
    service_ok: bool,
    /// Tunnels actifs, dans l'ordre de leur activation.
    active: Vec<String>,
    tunnels: Vec<String>,
    /// Version disponible, si une mise à jour a été trouvée.
    update: Option<String>,
}

#[derive(Default)]
struct TrayState {
    enabled: AtomicBool,
    last: Mutex<Option<Snapshot>>,
}

/// Résultat d'une action lancée depuis le menu, relayé à l'interface (toast).
#[derive(Clone, Serialize)]
struct TrayAction {
    ok: bool,
    message: String,
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let tunnels = Store::open().map(|s| s.names()).unwrap_or_default();
    let update = app
        .state::<UpdateState>()
        .available
        .lock()
        .ok()
        .and_then(|u| u.as_ref().map(|u| u.version.clone()));
    match status_from(client::request(&Request::Status)) {
        Ok(st) => Snapshot {
            service_ok: true,
            active: st.tunnels.into_iter().map(|t| t.name).collect(),
            tunnels,
            update,
        },
        Err(_) => Snapshot {
            service_ok: false,
            active: Vec::new(),
            tunnels,
            update,
        },
    }
}

impl Snapshot {
    fn is_active(&self, name: &str) -> bool {
        self.active.iter().any(|a| a == name)
    }
}

/// « usa », « usa et infra », « usa, infra et labo ».
fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} et {last}", rest.join(", ")),
    }
}

/// Action demandée depuis le menu.
enum Action {
    Connect(String),
    Disconnect(String),
    DisconnectAll,
}

fn build_menu(app: &AppHandle, snap: &Snapshot) -> tauri::Result<Menu<Wry>> {
    let status = match (snap.active.is_empty(), snap.service_ok) {
        (_, false) => "Service indisponible".to_string(),
        (false, true) => format!("● Connecté à {}", join_names(&snap.active)),
        (true, true) => "○ Déconnecté".to_string(),
    };
    let mut menu = MenuBuilder::new(app)
        .item(&MenuItem::with_id(app, "status", status, false, None::<&str>)?)
        .separator();
    if snap.tunnels.is_empty() {
        menu = menu.item(&MenuItem::with_id(app, "none", "Aucun tunnel", false, None::<&str>)?);
    }
    for name in &snap.tunnels {
        let checked = snap.is_active(name);
        menu = menu.item(&CheckMenuItem::with_id(
            app,
            format!("tunnel:{name}"),
            name,
            snap.service_ok,
            checked,
            None::<&str>,
        )?);
    }
    let disconnect = if snap.active.len() > 1 { "Tout déconnecter" } else { "Se déconnecter" };
    if let Some(version) = &snap.update {
        menu = menu.separator().item(&MenuItem::with_id(
            app,
            "update",
            format!("Mettre à jour vers la version {version}…"),
            true,
            None::<&str>,
        )?);
    }
    menu.separator()
        .item(&MenuItem::with_id(app, "disconnect", disconnect, !snap.active.is_empty(), None::<&str>)?)
        .item(&MenuItem::with_id(app, "show", "Ouvrir Cryptonaute", true, None::<&str>)?)
        .item(&MenuItem::with_id(app, "options", "Options…", true, None::<&str>)?)
        .separator()
        .item(&MenuItem::with_id(
            app,
            "quit",
            if snap.active.is_empty() { "Quitter" } else { "Quitter et déconnecter" },
            true,
            None::<&str>,
        )?)
        .build()
}

fn apply(app: &AppHandle, snap: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let (icon, tooltip) = if !snap.active.is_empty() {
        (ICON_ON, format!("Cryptonaute — connecté à {}", join_names(&snap.active)))
    } else if !snap.service_ok {
        (ICON_OFF, "Cryptonaute — service indisponible".to_string())
    } else {
        (ICON_OFF, "Cryptonaute — déconnecté".to_string())
    };
    if let Ok(img) = Image::from_bytes(icon) {
        let _ = tray.set_icon(Some(img));
    }
    let _ = tray.set_tooltip(Some(tooltip));
    if let Ok(menu) = build_menu(app, snap) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Relit l'état et met l'icône à jour si nécessaire (ou toujours si `force`).
fn refresh(app: &AppHandle, force: bool) {
    let state = app.state::<TrayState>();
    if !state.enabled.load(Ordering::SeqCst) {
        return;
    }
    let snap = snapshot(app);
    let mut last = state.last.lock().unwrap_or_else(|e| e.into_inner());
    if force || last.as_ref() != Some(&snap) {
        apply(app, &snap);
        *last = Some(snap);
    }
}

/// Met l'icône à jour sans attendre le prochain sondage (hors du thread appelant).
pub fn refresh_now(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || refresh(&app, true));
}

/// Vrai si l'icône est affichée (la fermeture de la fenêtre la masque alors
/// au lieu de quitter l'application).
pub fn is_enabled(app: &AppHandle) -> bool {
    app.state::<TrayState>().enabled.load(Ordering::SeqCst)
}

pub fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

fn run_action(app: &AppHandle, action: Action) {
    let app = app.clone();
    std::thread::spawn(move || {
        let result = match action {
            Action::Connect(name) => crate::connect_tunnel(&name).map(|_| format!("Connecté à « {name} »")),
            Action::Disconnect(name) => crate::disconnect_tunnel(Some(name.clone()))
                .map(|_| format!("« {name} » déconnecté")),
            Action::DisconnectAll => crate::disconnect_tunnel(None).map(|_| "Tunnels déconnectés".to_string()),
        };
        let action = match result {
            Ok(message) => TrayAction { ok: true, message },
            Err(message) => {
                // La fenêtre peut être masquée : on l'affiche pour que l'erreur soit vue.
                show_main(&app);
                TrayAction { ok: false, message }
            }
        };
        let _ = app.emit("tray-action", action);
        refresh(&app, true);
    });
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "show" => show_main(app),
        "options" => {
            show_main(app);
            let _ = app.emit("open-options", ());
        }
        "quit" => app.exit(0),
        "update" => {
            show_main(app);
            let _ = app.emit("open-update", ());
        }
        "disconnect" => run_action(app, Action::DisconnectAll),
        _ => {
            if let Some(name) = id.strip_prefix("tunnel:") {
                let active = app
                    .state::<TrayState>()
                    .last
                    .lock()
                    .ok()
                    .is_some_and(|s| s.as_ref().is_some_and(|s| s.is_active(name)));
                let name = name.to_string();
                run_action(app, if active { Action::Disconnect(name) } else { Action::Connect(name) });
            }
        }
    }
}

fn create(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_OFF)?)
        .tooltip("Cryptonaute")
        .menu(&build_menu(app, &Snapshot::default())?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Affiche ou retire l'icône de la zone de notification.
pub fn set_visible(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    let state = app.state::<TrayState>();
    state.enabled.store(visible, Ordering::SeqCst);
    *state.last.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if visible {
        create(app)?;
        let app = app.clone();
        std::thread::spawn(move || refresh(&app, true));
    } else {
        let _ = app.remove_tray_by_id(TRAY_ID);
    }
    Ok(())
}

pub fn setup(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    app.manage(TrayState::default());
    set_visible(app, visible)?;

    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL_INTERVAL);
        refresh(&app, false);
    });
    Ok(())
}

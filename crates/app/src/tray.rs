//! Icône de la zone de notification : état de connexion en un coup d'œil et
//! menu permettant de (dé)connecter un tunnel sans ouvrir la fenêtre.
//! L'utilisateur peut la masquer depuis les options de l'application.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use cryptonaute_common::ipc::{Request, TunnelState};
use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuBuilder, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::{client, status_from, store::Store};

const TRAY_ID: &str = "main";
const ICON_ON: &[u8] = include_bytes!("../icons/tray-on.png");
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png");
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Ce qui est affiché par l'icône ; elle n'est mise à jour que s'il change.
#[derive(Clone, Default, PartialEq)]
struct Snapshot {
    service_ok: bool,
    active: Option<String>,
    tunnels: Vec<String>,
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

fn snapshot() -> Snapshot {
    let tunnels = Store::open().map(|s| s.names()).unwrap_or_default();
    match status_from(client::request(&Request::Status)) {
        Ok(st) => Snapshot {
            service_ok: true,
            active: (st.state == TunnelState::Connected).then_some(st.tunnel).flatten(),
            tunnels,
        },
        Err(_) => Snapshot {
            service_ok: false,
            active: None,
            tunnels,
        },
    }
}

fn build_menu(app: &AppHandle, snap: &Snapshot) -> tauri::Result<Menu<Wry>> {
    let status = match (&snap.active, snap.service_ok) {
        (_, false) => "Service indisponible".to_string(),
        (Some(name), true) => format!("● Connecté à {name}"),
        (None, true) => "○ Déconnecté".to_string(),
    };
    let mut menu = MenuBuilder::new(app)
        .item(&MenuItem::with_id(app, "status", status, false, None::<&str>)?)
        .separator();
    if snap.tunnels.is_empty() {
        menu = menu.item(&MenuItem::with_id(app, "none", "Aucun tunnel", false, None::<&str>)?);
    }
    for name in &snap.tunnels {
        let checked = snap.active.as_deref() == Some(name.as_str());
        menu = menu.item(&CheckMenuItem::with_id(
            app,
            format!("tunnel:{name}"),
            name,
            snap.service_ok,
            checked,
            None::<&str>,
        )?);
    }
    menu.separator()
        .item(&MenuItem::with_id(app, "disconnect", "Se déconnecter", snap.active.is_some(), None::<&str>)?)
        .item(&MenuItem::with_id(app, "show", "Ouvrir Cryptonaute", true, None::<&str>)?)
        .item(&MenuItem::with_id(app, "options", "Options…", true, None::<&str>)?)
        .separator()
        .item(&MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?)
        .build()
}

fn apply(app: &AppHandle, snap: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let (icon, tooltip) = match &snap.active {
        Some(name) => (ICON_ON, format!("Cryptonaute — connecté à {name}")),
        None if !snap.service_ok => (ICON_OFF, "Cryptonaute — service indisponible".to_string()),
        None => (ICON_OFF, "Cryptonaute — déconnecté".to_string()),
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
    let snap = snapshot();
    let mut last = state.last.lock().unwrap_or_else(|e| e.into_inner());
    if force || last.as_ref() != Some(&snap) {
        apply(app, &snap);
        *last = Some(snap);
    }
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

fn run_action(app: &AppHandle, connect_to: Option<String>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let result = match &connect_to {
            Some(name) => crate::connect_tunnel(name).map(|_| format!("Connecté à « {name} »")),
            None => crate::disconnect_tunnel().map(|_| "Tunnel déconnecté".to_string()),
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
        "disconnect" => run_action(app, None),
        _ => {
            if let Some(name) = id.strip_prefix("tunnel:") {
                let active = app
                    .state::<TrayState>()
                    .last
                    .lock()
                    .ok()
                    .and_then(|s| s.as_ref().and_then(|s| s.active.clone()));
                if active.as_deref() == Some(name) {
                    run_action(app, None);
                } else {
                    run_action(app, Some(name.to_string()));
                }
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

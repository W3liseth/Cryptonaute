//! Icône de la zone de notification : état de connexion en un coup d'œil et
//! menu permettant de (dé)connecter les tunnels sans ouvrir la fenêtre (plusieurs
//! peuvent être actifs à la fois). L'utilisateur peut la masquer dans les options.
//!
//! - Windows : thread dédié avec sa propre boucle de messages ;
//! - Linux : protocole StatusNotifierItem (D-Bus), sans GTK ;
//! - macOS : thread principal (exigence d'AppKit), piloté par l'interface.
//!
//! Un thread interroge le service toutes les 2 s ; l'icône et le menu ne sont
//! reconstruits que si l'état affiché change.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::backend;
use crate::events;

const ICON_ON: &[u8] = include_bytes!("../icons/tray-on.png");
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png");
const POLL_INTERVAL: Duration = Duration::from_secs(2);

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Dernier état calculé par le thread d'interrogation.
static LATEST: Mutex<Option<Snapshot>> = Mutex::new(None);

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

fn snapshot() -> Snapshot {
    let b = backend::get();
    let tunnels = b.list().map(|l| l.into_iter().map(|t| t.name).collect()).unwrap_or_default();
    let update = crate::updater::available().map(|u| u.version);
    match b.status() {
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

/// Vrai si l'icône est affichée (fermer la fenêtre la masque alors au lieu de quitter).
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

/// Affiche ou retire l'icône.
pub fn set_visible(visible: bool) {
    ENABLED.store(visible, Ordering::SeqCst);
    refresh_soon();
}

/// Relit l'état sans attendre le prochain intervalle.
pub fn refresh_soon() {
    std::thread::spawn(|| {
        let snap = snapshot();
        *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(snap);
    });
}

fn icon(bytes: &[u8]) -> Option<tray_icon::Icon> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()
}

fn build_menu(snap: &Snapshot) -> Menu {
    let menu = Menu::new();
    let status = match (snap.active.is_empty(), snap.service_ok) {
        (_, false) => "Service indisponible".to_string(),
        (false, true) => format!("● Connecté à {}", join_names(&snap.active)),
        (true, true) => "○ Déconnecté".to_string(),
    };
    let sep = PredefinedMenuItem::separator;
    let _ = menu.append(&MenuItem::with_id("status", status, false, None));
    let _ = menu.append(&sep());
    if snap.tunnels.is_empty() {
        let _ = menu.append(&MenuItem::with_id("none", "Aucun tunnel", false, None));
    }
    for name in &snap.tunnels {
        let item = CheckMenuItem::with_id(format!("tunnel:{name}"), name, snap.service_ok, snap.is_active(name), None);
        let _ = menu.append(&item);
    }
    if let Some(version) = &snap.update {
        let _ = menu.append(&sep());
        let _ = menu.append(&MenuItem::with_id("update", format!("Mettre à jour vers la version {version}…"), true, None));
    }
    let _ = menu.append(&sep());
    let disconnect = if snap.active.len() > 1 { "Tout déconnecter" } else { "Se déconnecter" };
    let _ = menu.append(&MenuItem::with_id("disconnect", disconnect, !snap.active.is_empty(), None));
    let _ = menu.append(&MenuItem::with_id("show", "Ouvrir Cryptonaute", true, None));
    let _ = menu.append(&MenuItem::with_id("options", "Options…", true, None));
    let _ = menu.append(&sep());
    let quit = if snap.active.is_empty() { "Quitter" } else { "Quitter et déconnecter" };
    let _ = menu.append(&MenuItem::with_id("quit", quit, true, None));
    menu
}

fn tooltip(snap: &Snapshot) -> String {
    if !snap.active.is_empty() {
        format!("Cryptonaute — connecté à {}", join_names(&snap.active))
    } else if !snap.service_ok {
        "Cryptonaute — service indisponible".into()
    } else {
        "Cryptonaute — déconnecté".into()
    }
}

/// Action lancée depuis le menu : son résultat est affiché par l'interface.
enum Action {
    Connect(String),
    Disconnect(String),
    DisconnectAll,
}

fn run_action(action: Action) {
    std::thread::spawn(move || {
        let b = backend::get();
        let result = match action {
            Action::Connect(name) => b.connect(&name).map(|_| format!("Connecté à « {name} »")),
            Action::Disconnect(name) => b.disconnect(Some(&name)).map(|_| format!("« {name} » déconnecté")),
            Action::DisconnectAll => b.disconnect(None).map(|_| "Tunnels déconnectés".to_string()),
        };
        if result.is_err() {
            // La fenêtre peut être masquée : on l'affiche pour que l'erreur soit vue.
            crate::show_window();
        }
        events::post(move |app| match result {
            Ok(message) => app.toast(message, crate::ui::ToastKind::Success),
            Err(message) => app.toast_for(message, crate::ui::ToastKind::Error, 6.5),
        });
        refresh_soon();
    });
}

fn on_menu(id: &str) {
    match id {
        "show" => crate::show_window(),
        "options" => {
            crate::show_window();
            events::post(|app| app.open_options());
        }
        "update" => {
            crate::show_window();
            events::post(|app| app.open_update());
        }
        "quit" => crate::request_quit(),
        "disconnect" => run_action(Action::DisconnectAll),
        _ => {
            if let Some(name) = id.strip_prefix("tunnel:") {
                let active = LATEST
                    .lock()
                    .ok()
                    .and_then(|s| s.as_ref().map(|s| s.is_active(name)))
                    .unwrap_or(false);
                let name = name.to_string();
                run_action(if active { Action::Disconnect(name) } else { Action::Connect(name) });
            }
        }
    }
}

/// Icône et état affiché, propres au thread qui les a créés.
#[derive(Default)]
pub struct Driver {
    tray: Option<TrayIcon>,
    shown: Option<Snapshot>,
}

impl Driver {
    /// Crée, met à jour ou retire l'icône, et traite les clics.
    pub fn tick(&mut self) {
        if !is_enabled() {
            self.tray = None;
            self.shown = None;
        } else if let Some(snap) = LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let image = || icon(if snap.active.is_empty() { ICON_OFF } else { ICON_ON });
            match &self.tray {
                None => {
                    self.tray = TrayIconBuilder::new()
                        .with_id("main")
                        .with_tooltip(tooltip(&snap))
                        .with_menu(Box::new(build_menu(&snap)))
                        .with_menu_on_left_click(false)
                        .with_icon(image().unwrap_or_else(fallback_icon))
                        .build()
                        .map_err(|e| eprintln!("icône de notification : {e}"))
                        .ok();
                    self.shown = Some(snap);
                }
                Some(tray) if self.shown.as_ref() != Some(&snap) => {
                    tray.set_menu(Some(Box::new(build_menu(&snap))));
                    let _ = tray.set_tooltip(Some(tooltip(&snap)));
                    let _ = tray.set_icon(image());
                    self.shown = Some(snap);
                }
                Some(_) => {}
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            on_menu(event.id().as_ref());
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_window();
            }
        }
    }
}

fn fallback_icon() -> tray_icon::Icon {
    tray_icon::Icon::from_rgba([0x7c, 0x5c, 0xff, 0xff].repeat(16 * 16), 16, 16).expect("icône de repli valide")
}

/// Démarre l'interrogation de l'état et, hors macOS, le thread de l'icône.
pub fn setup(visible: bool) {
    ENABLED.store(visible, Ordering::SeqCst);
    std::thread::spawn(|| loop {
        let snap = snapshot();
        *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(snap);
        std::thread::sleep(POLL_INTERVAL);
    });
    #[cfg(not(target_os = "macos"))]
    std::thread::spawn(|| {
        let mut driver = Driver::default();
        loop {
            pump_messages();
            driver.tick();
            std::thread::sleep(Duration::from_millis(40));
        }
    });
}

/// Windows : l'icône reçoit ses clics par la file de messages du thread.
#[cfg(windows)]
fn pump_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
    // SAFETY: structure MSG locale, remplie par PeekMessageW.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn pump_messages() {}

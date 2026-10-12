//! Interface graphique de Cryptonaute (aucun droit administrateur requis).
//!
//! Les configurations sont gérées localement (stockage chiffré par utilisateur) ;
//! l'activation des tunnels est déléguée au service privilégié via IPC.
//! L'interface est native (egui) : ni navigateur intégré ni JavaScript.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod backend;
mod client;
mod events;
mod instance;
mod paths;
mod session;
mod settings;
mod store;
mod tray;
mod ui;
mod updater;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use eframe::egui::{self, ViewportBuilder, ViewportCommand};

/// Délai avant la première recherche de mise à jour, puis intervalle entre deux recherches.
const UPDATE_FIRST_CHECK: Duration = Duration::from_secs(15);
const UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 3600);

/// L'utilisateur a demandé à quitter (la fermeture n'est alors plus annulée).
static QUITTING: AtomicBool = AtomicBool::new(false);
/// Le moteur de rendu a démarré (fenêtre et contexte graphique créés).
static UI_STARTED: AtomicBool = AtomicBool::new(false);

/// Quitte l'application ; les tunnels sont déconnectés à la sortie (voir `session`).
pub fn request_quit() {
    QUITTING.store(true, Ordering::SeqCst);
    match events::context() {
        Some(ctx) => {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            ctx.request_repaint();
        }
        None => {
            session::close();
            std::process::exit(0);
        }
    }
}

/// Relance l'application (après une mise à jour installée sous Linux).
pub fn restart() {
    spawn_successor(std::iter::empty::<&str>(), None);
    request_quit();
}

/// Lance une nouvelle instance, qui attend la fin de celle-ci (instance unique).
fn spawn_successor<S: AsRef<std::ffi::OsStr>>(args: impl IntoIterator<Item = S>, renderer: Option<&str>) {
    if let Ok(exe) = std::env::current_exe() {
        let mut command = std::process::Command::new(exe);
        command.args(args).env("CRYPTONAUTE_WAIT_PID", std::process::id().to_string());
        if let Some(renderer) = renderer {
            command.env("CRYPTONAUTE_RENDERER", renderer);
        }
        let _ = command.spawn();
    }
}

/// Affiche la fenêtre au premier plan (icône de notification, second lancement).
pub fn show_window() {
    if let Some(ctx) = events::context() {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

/// Bouton « Fermer » : masque la fenêtre si l'icône de notification est affichée,
/// sinon quitte l'application.
pub fn close_window(ctx: &egui::Context) {
    if tray::is_enabled() {
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    } else {
        request_quit();
    }
}

/// Fermeture par le système (Alt+F4, bouton natif sous macOS) : même règle.
pub fn handle_close_request(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().close_requested()) && !QUITTING.load(Ordering::SeqCst) {
        if tray::is_enabled() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        } else {
            QUITTING.store(true, Ordering::SeqCst);
        }
    }
    #[cfg(target_os = "macos")]
    mac_tray::tick(ctx);
}

/// macOS : l'icône de notification vit sur le thread principal.
#[cfg(target_os = "macos")]
mod mac_tray {
    use std::cell::RefCell;

    thread_local! {
        static DRIVER: RefCell<crate::tray::Driver> = RefCell::new(crate::tray::Driver::default());
    }

    pub fn tick(ctx: &eframe::egui::Context) {
        DRIVER.with(|d| d.borrow_mut().tick());
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn spawn_update_checks() {
    std::thread::spawn(|| {
        std::thread::sleep(UPDATE_FIRST_CHECK);
        loop {
            if settings::load().auto_update {
                match updater::check() {
                    Ok(found) => {
                        if updater::store(found.clone()) {
                            tray::refresh_soon();
                            events::post(move |app| app.on_update_found(found, true));
                        }
                    }
                    Err(e) => eprintln!("recherche de mise à jour : {e}"),
                }
            }
            std::thread::sleep(UPDATE_INTERVAL);
        }
    });
}

fn window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(include_bytes!("../icons/icon.png"), image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result {
    instance::wait_for_previous();
    if !instance::acquire() {
        return Ok(());
    }
    let start_hidden = std::env::args().any(|a| a == "--minimized");
    // Reprise des données de RustGuard, l'ancien nom de l'application.
    paths::migrate_legacy_data();
    autostart::repair();
    let prefs = settings::load();
    // En démonstration (développement), rien ne touche au vrai service.
    if backend::is_demo() {
        tray::set_visible(prefs.show_tray);
    } else {
        tray::setup(prefs.show_tray);
        spawn_update_checks();
        session::spawn();
    }

    // Sans icône de notification, une fenêtre masquée serait inaccessible.
    let visible = !start_hidden || !prefs.show_tray;
    let mut viewport = ViewportBuilder::default()
        .with_title("Cryptonaute")
        .with_app_id("cryptonaute")
        .with_inner_size([1120.0, 740.0])
        .with_min_inner_size([900.0, 620.0])
        .with_visible(visible)
        .with_drag_and_drop(true);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    // macOS : boutons natifs (feux tricolores) superposés à la barre de titre ;
    // ailleurs : fenêtre sans décoration, avec barre de titre et bords maison.
    viewport = if cfg!(target_os = "macos") {
        viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false)
    } else {
        viewport.with_decorations(false)
    };
    let mut options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    let renderer = renderer();
    options.renderer = renderer;
    // Sans synchronisation verticale : la présentation d'une image ne bloque plus le
    // thread de la fenêtre, qui suit alors les déplacements sans à-coups. La cadence
    // reste limitée par l'interface (voir `ui::FRAME`).
    options.glow_options.vsync = false;
    options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    #[cfg(debug_assertions)]
    if renderer == eframe::Renderer::Glow && std::env::var_os("CRYPTONAUTE_TEST_GL_FAILURE").is_some() {
        options.depth_buffer = u8::MAX; // aucune configuration OpenGL ne convient
    }

    // Le moteur de rendu peut échouer par une erreur ou une panique d'eframe.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eframe::run_native(
            "Cryptonaute",
            options,
            Box::new(|cc| {
                UI_STARTED.store(true, Ordering::SeqCst);
                ui::theme::install_fonts(&cc.egui_ctx);
                ui::theme::install_style(&cc.egui_ctx);
                egui_extras::install_image_loaders(&cc.egui_ctx);
                events::set_context(cc.egui_ctx.clone());
                Ok(Box::new(ui::App::new(&cc.egui_ctx)))
            }),
        )
    }));
    // OpenGL indisponible (machine virtuelle, bureau à distance, pilote trop ancien) :
    // l'interface n'a pas pu démarrer, on relance avec wgpu (DirectX 12, Vulkan…),
    // sans déconnecter les tunnels.
    if renderer == eframe::Renderer::Glow && !UI_STARTED.load(Ordering::SeqCst) && !matches!(outcome, Ok(Ok(()))) {
        eprintln!("OpenGL indisponible : relance avec wgpu");
        spawn_successor(std::env::args_os().skip(1), Some("wgpu"));
        return Ok(());
    }
    let result = outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    // Quitter l'application déconnecte les tunnels (s'il n'y a pas d'autre
    // application ouverte).
    if !backend::is_demo() {
        session::close();
    }
    result
}

/// Moteur de rendu : OpenGL (glow), le plus économe en mémoire, sauf sous macOS
/// (Metal, via wgpu). `CRYPTONAUTE_RENDERER=wgpu` ou `glow` force le choix.
fn renderer() -> eframe::Renderer {
    match std::env::var("CRYPTONAUTE_RENDERER").as_deref() {
        Ok("wgpu") => eframe::Renderer::Wgpu,
        Ok("glow") => eframe::Renderer::Glow,
        _ if cfg!(target_os = "macos") => eframe::Renderer::Wgpu,
        _ => eframe::Renderer::Glow,
    }
}

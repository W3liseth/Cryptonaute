//! Développement uniquement : scènes prédéfinies et capture d'écran
//! automatique, pour vérifier le rendu sans manipulation.
//!
//! `CRYPTONAUTE_DEMO=1 CRYPTONAUTE_SCENE=connected CRYPTONAUTE_SCREENSHOT=capture.png`
//! ouvre la scène, attend qu'elle soit stable, enregistre la fenêtre puis quitte.

use eframe::egui::{self, UserData, ViewportCommand};

use super::app::{App, ToastKind};

pub struct Scene {
    name: String,
    output: Option<String>,
    step: u32,
    started: f64,
    requested: bool,
}

impl Scene {
    pub fn from_env() -> Option<Self> {
        if !cfg!(debug_assertions) {
            return None;
        }
        let name = std::env::var("CRYPTONAUTE_SCENE").ok();
        let output = std::env::var("CRYPTONAUTE_SCREENSHOT").ok();
        if name.is_none() && output.is_none() {
            return None;
        }
        Some(Scene {
            name: name.unwrap_or_else(|| "main".into()),
            output,
            step: 0,
            started: 0.0,
            requested: false,
        })
    }

    /// Fait avancer la scène ; renvoie vrai quand la capture est enregistrée.
    pub fn tick(app: &mut App, ctx: &egui::Context) {
        let Some(mut scene) = app.scene.take() else { return };
        if scene.step == 0 {
            scene.started = app.now;
            scene.setup(app);
        }
        scene.step += 1;
        let elapsed = app.now - scene.started;
        let delay = match scene.name.as_str() {
            "connected" | "multi" | "scrolled" | "hide-show" => 6.0,
            _ => 2.5,
        };
        if scene.name == "multi" && scene.step > 1 && elapsed > 2.2 && !app.is_active("homelab") && app.pending.is_none() {
            app.select("homelab");
            app.toggle_power();
        }
        if let Some(path) = &scene.output {
            if elapsed > delay && !scene.requested {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                scene.requested = true;
            }
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let [w, h] = image.size;
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    let _ = img.save(path);
                }
                crate::request_quit();
            }
        }
        ctx.request_repaint();
        app.scene = Some(scene);
    }

    fn setup(&self, app: &mut App) {
        match self.name.as_str() {
            "connected" => {
                app.select("bureau-paris");
                app.toggle_power();
            }
            "multi" => {
                app.select("bureau-paris");
                app.toggle_power();
            }
            "scrolled" => {
                app.select("bureau-paris");
                app.toggle_power();
                app.scroll_to_bottom = true;
            }
            "editor" => {
                app.select("bureau-paris");
                app.edit_tunnel();
            }
            "new" => app.new_tunnel(),
            "confirm" => app.ask_delete(),
            "options" => app.open_options(),
            "notes" => {
                app.open_options();
                app.open_patch_notes();
            }
            "update" => {
                let info = crate::updater::UpdateInfo::demo();
                crate::updater::store(Some(info.clone()));
                app.on_update_found(Some(info), false);
                app.open_update();
            }
            "hide-show" => {
                // Masque la fenêtre puis la réaffiche depuis un autre thread,
                // comme le fait l'icône de notification.
                if let Some(ctx) = crate::events::context() {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                }
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    crate::show_window();
                    crate::events::post(|app| app.toast("Fenêtre réaffichée", ToastKind::Info));
                });
            }
            "toasts" => {
                app.toast("Connecté à « bureau-paris »", ToastKind::Success);
                app.toast("Le service Cryptonaute n'est pas démarré. Réinstallez l'application.", ToastKind::Error);
                app.toast("Tunnel « homelab » déconnecté", ToastKind::Info);
            }
            _ => {}
        }
    }
}

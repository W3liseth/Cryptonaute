//! Interface native (egui), qui reproduit l'ancienne interface web : thème
//! sombre « verre dépoli », accents animés selon l'état, liste des tunnels,
//! carte de connexion, statistiques, graphique et fenêtres modales.

mod app;
mod changelog;
mod chart;
mod debug;
mod dialogs;
mod format;
mod highlight;
pub mod icons;
mod overlays;
mod paint;
mod power;
pub mod theme;
mod view;
mod widgets;

use std::time::Duration;

use eframe::egui::{self, CursorIcon, Id, Key, Modifiers, Pos2, Rect, ResizeDirection, Sense, Vec2, ViewportCommand};

pub use app::{App, ToastKind};
use app::Modal;
use theme::Accent;

/// Intervalle entre deux images pour les animations continues (fond, ondes…).
const FRAME: Duration = Duration::from_millis(33);

impl eframe::App for App {
    /// Appelé à chaque passage, y compris fenêtre masquée (icône de notification).
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.now = format::now_secs();
        self.drain_jobs();
        crate::handle_close_request(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG.to_normalized_gamma_f32()
    }
}

impl App {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let accent = Accent::animate(&ctx, self.mood());
        let time = ui.input(|i| i.time);
        paint::backdrop(ui.painter(), screen, accent, time);

        let title = Rect::from_min_size(screen.min, Vec2::new(screen.width(), view::TITLEBAR_H));
        self.titlebar(ui, title, accent);
        let body = Rect::from_min_max(Pos2::new(screen.left(), title.bottom()), screen.max);
        let sidebar = Rect::from_min_size(body.min, Vec2::new(view::SIDEBAR_W, body.height()));
        self.sidebar(ui, sidebar, accent);
        let main = Rect::from_min_max(Pos2::new(sidebar.right(), body.top()), body.max);
        self.main_area(ui, main, accent);

        dialogs::show_modals(self, &ctx, accent);
        overlays::drop_overlay(self, &ctx, accent);
        overlays::toasts(self, &ctx);
        resize_handles(ui, screen);
        self.keyboard(&ctx);
        self.dropped_files(&ctx);
        debug::Scene::tick(self, &ctx);
        ctx.request_repaint_after(FRAME);
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let pressed = |key: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key));
        let shortcut = |key: Key| ctx.input_mut(|i| i.consume_key(cmd, key));

        if pressed(Key::Escape) {
            // Les patch notes s'ouvrent par-dessus les options : on ne ferme qu'elles.
            self.close_top_modal();
            return;
        }
        let modal_open = self.has_modal();
        if shortcut(Key::Comma) && !modal_open {
            self.open_options();
            return;
        }
        if shortcut(Key::S) && self.has_modal_of(|m| matches!(m, Modal::Editor(_))) {
            self.save_editor();
            return;
        }
        if modal_open || ctx.egui_wants_keyboard_input() {
            return;
        }
        if shortcut(Key::N) {
            self.new_tunnel();
        } else if shortcut(Key::O) {
            self.pick_import_files();
        } else if pressed(Key::Delete) {
            self.ask_delete();
        } else if pressed(Key::Enter) || pressed(Key::Space) {
            self.toggle_power();
        } else if pressed(Key::ArrowDown) {
            self.select_relative(1);
        } else if pressed(Key::ArrowUp) {
            self.select_relative(-1);
        }
    }

    fn dropped_files(&mut self, ctx: &egui::Context) {
        let files: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !files.is_empty() {
            self.import_files(files);
        }
    }
}

/// Bords de redimensionnement de la fenêtre sans décoration (6 px).
fn resize_handles(ui: &egui::Ui, screen: Rect) {
    if cfg!(target_os = "macos") || ui.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    const B: f32 = 6.0;
    let zones = [
        (Rect::from_min_max(screen.min, screen.min + Vec2::splat(B)), ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest),
        (Rect::from_min_max(Pos2::new(screen.right() - B, screen.top()), Pos2::new(screen.right(), screen.top() + B)), ResizeDirection::NorthEast, CursorIcon::ResizeNorthEast),
        (Rect::from_min_max(Pos2::new(screen.left(), screen.bottom() - B), Pos2::new(screen.left() + B, screen.bottom())), ResizeDirection::SouthWest, CursorIcon::ResizeSouthWest),
        (Rect::from_min_max(screen.max - Vec2::splat(B), screen.max), ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast),
        (Rect::from_min_max(Pos2::new(screen.left() + B, screen.top()), Pos2::new(screen.right() - B, screen.top() + 3.0)), ResizeDirection::North, CursorIcon::ResizeVertical),
        (Rect::from_min_max(Pos2::new(screen.left() + B, screen.bottom() - B), Pos2::new(screen.right() - B, screen.bottom())), ResizeDirection::South, CursorIcon::ResizeVertical),
        (Rect::from_min_max(Pos2::new(screen.left(), screen.top() + B), Pos2::new(screen.left() + B, screen.bottom() - B)), ResizeDirection::West, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(Pos2::new(screen.right() - B, screen.top() + B), Pos2::new(screen.right(), screen.bottom() - B)), ResizeDirection::East, CursorIcon::ResizeHorizontal),
    ];
    for (i, (rect, dir, cursor)) in zones.into_iter().enumerate() {
        let resp = ui.interact(rect, Id::new(("resize", i)), Sense::drag()).on_hover_cursor(cursor);
        if resp.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}

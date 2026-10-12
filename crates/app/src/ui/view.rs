//! Fenêtre principale : barre de titre, barre latérale (liste des tunnels) et
//! zone principale (écran d'accueil ou détail du tunnel sélectionné).

use eframe::egui::{
    self, Align, Color32, CursorIcon, Id, Layout, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2, ViewportCommand,
};
use eframe::epaint::RectShape;

use super::app::{App, PendingKind};
use super::icons::{self, Icon};
use super::paint::{self, cr, WithOpacity};
use super::theme::{self, Accent, Mood, Weight};
use super::widgets::{self, Button, ChipKind, Kind, Size, TextField};
use super::{chart, format, power};

pub const TITLEBAR_H: f32 = 40.0;
pub const SIDEBAR_W: f32 = 300.0;
const MACOS: bool = cfg!(target_os = "macos");

impl App {
    // ---------- Barre de titre ----------

    pub fn titlebar(&mut self, ui: &mut Ui, rect: Rect, accent: Accent) {
        let ctx = ui.ctx().clone();
        let controls_w = if MACOS { 0.0 } else { 46.0 * 3.0 };
        let drag = Rect::from_min_max(rect.min, Pos2::new(rect.right() - controls_w, rect.bottom()));
        let resp = ui.interact(drag, Id::new("titlebar-drag"), Sense::click_and_drag());
        if resp.drag_started_by(PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }

        // Logo et nom (sous macOS, après les boutons natifs superposés).
        let left = rect.left() + if MACOS { 84.0 } else { 16.0 };
        let logo = Rect::from_center_size(Pos2::new(left + 10.0, rect.center().y), Vec2::splat(20.0));
        ui.painter().add(paint::soft_disc(logo.center(), 6.0, 10.0, theme::fade(accent.a1, 0.6), true));
        icons::paint_svg(ui, "bytes://logo.svg", icons::LOGO_SVG, logo, Color32::WHITE);
        let mut name = theme::job("Cryptonaute", 14.0, Weight::Semibold, theme::TEXT);
        name.sections[0].format.extra_letter_spacing = 0.2;
        let galley = ui.painter().layout_job(name);
        ui.painter().galley(Pos2::new(logo.right() + 10.0, rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);

        if MACOS {
            return;
        }
        let buttons = [
            (Icon::WinMinimize, "Réduire", false),
            (Icon::WinMaximize, "Agrandir", false),
            (Icon::WinClose, self.close_title(), true),
        ];
        for (i, (icon, tip, close)) in buttons.into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(rect.right() - controls_w + 46.0 * i as f32, rect.top()), Vec2::new(46.0, rect.height()));
            let resp = ui.interact(r, Id::new(("win", i)), Sense::click()).on_hover_text(tip);
            let hover = widgets::hover_anim(ui, resp.id, resp.hovered());
            let (bg, fg) = if close {
                (theme::fade(Color32::from_rgb(0xe8, 0x11, 0x23), hover), theme::lerp(theme::TEXT_2, Color32::WHITE, hover))
            } else {
                (theme::fade(theme::SURFACE_2, hover), theme::lerp(theme::TEXT_2, theme::TEXT, hover))
            };
            ui.painter().rect_filled(r, 0.0, bg);
            icons::paint_centered(ui, icon, r.center(), 12.0, fg);
            if resp.clicked() {
                match i {
                    0 => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                    1 => {
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    }
                    _ => crate::close_window(&ctx),
                }
            }
        }
    }

    fn close_title(&self) -> &'static str {
        if self.options.show_tray {
            "Fermer (Cryptonaute reste dans la zone de notification)"
        } else {
            "Quitter (les tunnels seront déconnectés)"
        }
    }

    // ---------- Barre latérale ----------

    pub fn sidebar(&mut self, ui: &mut Ui, rect: Rect, accent: Accent) {
        // Fond légèrement éclairci et bordure droite.
        let bg = paint::linear(rect, 180.0, vec![(0.0, theme::white_alpha(0.025)), (1.0, theme::white_alpha(0.01))]);
        ui.painter().add(paint::gradient_rect(ui.ctx(), rect, 0.0, &bg));
        ui.painter().vline(rect.right() - 0.5, rect.y_range(), Stroke::new(1.0, theme::BORDER));

        let inner = Rect::from_min_max(rect.min + Vec2::new(16.0, 10.0), rect.max - Vec2::new(14.0, 14.0));
        let gap = 14.0;
        let foot_h = 40.0;
        let banner_h = if self.update.is_some() { 48.0 + gap } else { 0.0 };
        let actions_h = 38.0;
        let mut y = inner.top();

        // En-tête : « TUNNELS » et compteur
        let head = theme::caps("Tunnels", 13.0, 1.6, theme::MUTED);
        let galley = ui.painter().layout_job(head);
        let head_h = 20.0;
        ui.painter().galley(Pos2::new(inner.left() + 4.0, y + (head_h - galley.size().y) / 2.0), galley.clone(), theme::MUTED);
        let count = ui.painter().layout_job(theme::job(self.tunnels.len().to_string(), 11.0, Weight::Bold, theme::TEXT_2));
        let pill = Rect::from_min_size(
            Pos2::new(inner.left() + 4.0 + galley.size().x + 10.0, y + (head_h - 18.0) / 2.0),
            Vec2::new(count.size().x + 16.0, 18.0),
        );
        ui.painter().rect_filled(pill, cr(9.0), theme::SURFACE_2);
        ui.painter().galley(pill.center() - count.size() / 2.0, count, theme::TEXT_2);
        y += head_h + gap;

        // Recherche
        let search_rect = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 38.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(search_rect));
        TextField {
            id: Id::new("search"),
            text: &mut self.filter,
            hint: "Rechercher…",
            height: 38.0,
            radius: 12.0,
            font_size: 14.0,
            fill: theme::SURFACE,
            invalid: false,
            icon: Some(Icon::Search),
            char_limit: 64,
        }
        .show(&mut child, accent);
        y += 38.0 + gap;

        // Liste
        let list_bottom = inner.bottom() - foot_h - gap - banner_h - actions_h - gap;
        let list_rect = Rect::from_min_max(Pos2::new(inner.left(), y), Pos2::new(inner.right(), list_bottom));
        self.tunnel_list(ui, list_rect, accent);
        y = list_bottom + gap;

        // Actions : Importer / Nouveau
        let half = (inner.width() - 8.0) / 2.0;
        let mut row = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), actions_h))).layout(Layout::left_to_right(Align::Center)));
        row.spacing_mut().item_spacing.x = 8.0;
        if Button::new("Importer").icon(Icon::Import).kind(Kind::Ghost).width(half).show(&mut row, accent).on_hover_text("Importer un fichier .conf (Ctrl+O)").clicked() {
            self.pick_import_files();
        }
        if Button::new("Nouveau").icon(Icon::Plus).kind(Kind::Primary).width(half).show(&mut row, accent).on_hover_text("Nouveau tunnel (Ctrl+N)").clicked() {
            self.new_tunnel();
        }
        y += actions_h + gap;

        // Bandeau de mise à jour
        if let Some(version) = self.update.as_ref().map(|u| u.version.clone()) {
            let r = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 48.0));
            if self.update_banner(ui, r, &version) {
                self.open_update();
            }
        }

        // Pied : état du service et bouton des options
        let foot = Rect::from_min_max(Pos2::new(inner.left(), inner.bottom() - foot_h), inner.max);
        let pill = Rect::from_min_max(foot.min, Pos2::new(foot.right() - 48.0, foot.bottom()));
        self.service_pill(ui, pill);
        let opt = Rect::from_min_max(Pos2::new(foot.right() - 40.0, foot.top()), foot.max);
        if self.options_button(ui, opt) {
            self.open_options();
        }
    }

    fn tunnel_list(&mut self, ui: &mut Ui, rect: Rect, accent: Accent) {
        let q = self.filter.to_lowercase();
        let items: Vec<String> = self.tunnels.iter().filter(|t| t.name.to_lowercase().contains(&q)).map(|t| t.name.clone()).collect();
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        if items.is_empty() {
            let text = if self.tunnels.is_empty() { "Aucun tunnel pour le moment" } else { "Aucun résultat" };
            let galley = child.painter().layout_job(theme::job(text, 13.0, Weight::Regular, theme::MUTED));
            child.painter().galley(Pos2::new(rect.center().x - galley.size().x / 2.0, rect.top() + 30.0), galley, theme::MUTED);
            return;
        }
        let now = self.now;
        for (i, name) in items.iter().enumerate() {
            self.list_appear.entry(name.clone()).or_insert(now + i as f64 * 0.035);
        }
        egui::ScrollArea::vertical().id_salt("tunnel-list").auto_shrink([false, false]).show(&mut child, |ui| {
            ui.add_space(2.0);
            for name in &items {
                self.tunnel_item(ui, name, accent);
                ui.add_space(6.0);
            }
        });
    }

    fn tunnel_item(&mut self, ui: &mut Ui, name: &str, accent: Accent) {
        let Some(t) = self.tunnels.iter().find(|t| t.name == name).cloned() else { return };
        let width = ui.available_width() - 4.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width + 2.0, 62.0), Sense::click());
        let rect = Rect::from_min_size(rect.min + Vec2::new(2.0, 0.0), Vec2::new(width, 62.0));
        // Entrée animée (`slideIn`)
        let appear = self.list_appear.get(name).copied().unwrap_or(0.0);
        let p = paint::ease_out(((self.now - appear) / 0.45).clamp(0.0, 1.0) as f32);
        let hover = widgets::hover_anim(ui, resp.id, resp.hovered());
        let rect = rect.translate(Vec2::new(-12.0 * (1.0 - p) + 2.0 * paint::spring(hover), 0.0));
        let painter = ui.painter().clone().with_opacity(p);

        let selected = self.selected.as_deref() == Some(name);
        let active = self.is_active(name);
        let busy = self.pending.as_ref().is_some_and(|p| p.name == name);
        if selected {
            painter.add(RectShape::new(rect, cr(14.0), theme::SURFACE_2, Stroke::new(1.0, theme::BORDER_STRONG), StrokeKind::Inside));
            let bar = Rect::from_min_max(Pos2::new(rect.left() - 2.0, rect.top() + 14.0), Pos2::new(rect.left() + 1.0, rect.bottom() - 14.0));
            painter.add(paint::shadow(bar, 1.5, Vec2::ZERO, 12.0, 0.0, theme::fade(accent.a1, 0.9)));
            let brush = paint::linear(bar, 180.0, vec![(0.0, accent.a1), (1.0, accent.a2)]);
            painter.add(paint::gradient_rect(ui.ctx(), bar, 1.5, &brush));
        } else if hover > 0.0 {
            painter.rect_filled(rect, cr(14.0), theme::fade(theme::SURFACE, hover));
        }

        // Pastille de couleur avec l'initiale
        let avatar = Rect::from_min_size(rect.min + Vec2::new(12.0, 12.0), Vec2::splat(38.0));
        let h = format::hue(name);
        let brush = paint::linear(avatar, 135.0, vec![(0.0, theme::hsl(h, 0.7, 0.55, 0.9)), (1.0, theme::hsl(h + 50.0, 0.8, 0.5, 0.9))]);
        painter.add(paint::gradient_rect(ui.ctx(), avatar, 11.0, &brush));
        painter.add(paint::top_highlight(avatar, 6.0, theme::white_alpha(0.25)));
        let initial: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
        let g = painter.layout_job(theme::job(initial, 15.0, Weight::Bold, Color32::WHITE));
        painter.galley(avatar.center() - g.size() / 2.0, g, Color32::WHITE);

        // Nom et sous-titre
        let text_left = avatar.right() + 12.0;
        let text_w = rect.right() - 12.0 - 9.0 - 12.0 - text_left;
        let sub = if t.error.is_some() {
            "Configuration invalide".to_string()
        } else if busy {
            if self.pending.as_ref().is_some_and(|p| p.kind == PendingKind::Connect) { "Connexion…".into() } else { "Déconnexion…".into() }
        } else if active {
            format!("Connecté · {} ↓", format::rate(self.last_rate(name).0))
        } else {
            t.summary.as_ref().and_then(|s| s.peers.first()).and_then(|p| p.endpoint.clone()).unwrap_or_else(|| "—".into())
        };
        let sub_color = if t.error.is_some() { theme::ROSE } else if active { theme::TEAL } else { theme::MUTED };
        let mut name_job = theme::job(name, 14.0, Weight::Semibold, theme::TEXT);
        name_job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
        let mut sub_job = theme::job(sub, 12.0, Weight::Regular, sub_color);
        sub_job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
        let ng = painter.layout_job(name_job);
        let sg = painter.layout_job(sub_job);
        let block = ng.size().y + 2.0 + sg.size().y;
        let top = rect.center().y - block / 2.0;
        painter.galley(Pos2::new(text_left, top), ng.clone(), theme::TEXT);
        painter.galley(Pos2::new(text_left, top + ng.size().y + 2.0), sg, sub_color);

        // Point d'état
        let dot = Pos2::new(rect.right() - 12.0 - 4.5, rect.center().y);
        let time = ui.input(|i| i.time);
        if active {
            let phase = (time / 1.8).fract() as f32;
            let s = 0.6 + 1.2 * paint::ease_out(phase);
            painter.circle_stroke(dot, 8.5 * s, Stroke::new(2.0, theme::fade(theme::GREEN, 0.9 * (1.0 - phase))));
            painter.add(paint::soft_disc(dot, 4.5, 10.0, theme::fade(theme::GREEN, 0.8), true));
            painter.circle_filled(dot, 4.5, theme::GREEN);
        } else if busy {
            let blink = 0.4 + 0.6 * paint::ping_pong(time, 1.6);
            painter.circle_filled(dot, 4.5, theme::fade(theme::CYAN, blink));
        } else {
            painter.circle_filled(dot, 4.5, Color32::from_rgb(0x3a, 0x3e, 0x5c));
        }

        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() {
            self.select(name);
        }
        if resp.double_clicked() {
            self.select(name);
            self.toggle_power();
        }
    }

    /// Bandeau « Mise à jour disponible » ; vrai s'il a été cliqué.
    fn update_banner(&self, ui: &mut Ui, rect: Rect, version: &str) -> bool {
        let resp = ui.interact(rect, Id::new("update-banner"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let hover = widgets::hover_anim(ui, resp.id, resp.hovered());
        let rect = rect.translate(Vec2::new(0.0, -hover));
        let lift = 1.0 + 0.15 * hover;
        let tint = |c: Color32| theme::fade(c, lift);
        let brush = paint::linear(rect, 135.0, vec![(0.0, tint(theme::rgba(16, 185, 129, 0.16))), (1.0, tint(theme::rgba(34, 211, 238, 0.10)))]);
        ui.painter().add(paint::gradient_rect(ui.ctx(), rect, 14.0, &brush));
        ui.painter().add(RectShape::stroke(rect, cr(14.0), Stroke::new(1.0, theme::rgba(16, 185, 129, 0.35)), StrokeKind::Inside));
        let icon = Rect::from_min_size(Pos2::new(rect.left() + 12.0, rect.center().y - 15.0), Vec2::splat(30.0));
        ui.painter().rect_filled(icon, cr(10.0), theme::rgba(16, 185, 129, 0.2));
        icons::paint_centered(ui, Icon::Update, icon.center(), 17.0, theme::GREEN);
        let title = ui.painter().layout_job(theme::job("Mise à jour disponible", 13.0, Weight::Semibold, theme::TEXT));
        let mut sub_job = theme::job(format!("Version {version} · cliquez pour l’installer"), 12.0, Weight::Regular, theme::TEXT_2);
        sub_job.wrap = egui::text::TextWrapping::truncate_at_width(rect.right() - icon.right() - 22.0);
        let sub = ui.painter().layout_job(sub_job);
        let block = title.size().y + sub.size().y;
        let top = rect.center().y - block / 2.0;
        ui.painter().galley(Pos2::new(icon.right() + 10.0, top), title.clone(), theme::TEXT);
        ui.painter().galley(Pos2::new(icon.right() + 10.0, top + title.size().y), sub, theme::TEXT_2);
        resp.clicked()
    }

    fn service_pill(&self, ui: &mut Ui, rect: Rect) {
        let resp = ui.interact(rect, Id::new("service-pill"), Sense::hover());
        ui.painter().add(RectShape::new(rect, cr(12.0), theme::SURFACE, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside));
        let (color, label) = match self.service.available {
            None => (theme::AMBER, "Connexion au service…".to_string()),
            Some(true) => (
                theme::GREEN,
                format!("Service actif{}", self.service.version.as_ref().map(|v| format!(" · v{v}")).unwrap_or_default()),
            ),
            Some(false) => (theme::ROSE, "Service indisponible".to_string()),
        };
        let dot = Pos2::new(rect.left() + 12.0 + 4.0, rect.center().y);
        let alpha = if self.service.available.is_none() { 0.4 + 0.6 * paint::ping_pong(ui.input(|i| i.time), 2.0) } else { 1.0 };
        ui.painter().add(paint::soft_disc(dot, 4.0, 8.0, theme::fade(color, 0.7 * alpha), true));
        ui.painter().circle_filled(dot, 4.0, theme::fade(color, alpha));
        let mut job = theme::job(label, 12.0, Weight::Regular, theme::TEXT_2);
        job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 42.0);
        let g = ui.painter().layout_job(job);
        ui.painter().galley(Pos2::new(dot.x + 14.0, rect.center().y - g.size().y / 2.0), g, theme::TEXT_2);
        if let Some(err) = &self.service.error {
            resp.on_hover_text(err);
        }
    }

    /// Bouton des options, dont l'engrenage tourne au survol.
    fn options_button(&self, ui: &mut Ui, rect: Rect) -> bool {
        let resp = ui.interact(rect, Id::new("options-btn"), Sense::click()).on_hover_text("Options (Ctrl+,)").on_hover_cursor(CursorIcon::PointingHand);
        let hover = ui.ctx().animate_bool_with_time(resp.id.with("rot"), resp.hovered(), 0.6);
        let lift = widgets::hover_anim(ui, resp.id, resp.hovered());
        let r = rect.translate(Vec2::new(0.0, -lift));
        ui.painter().add(RectShape::new(r, cr(12.0), theme::lerp(theme::SURFACE, theme::SURFACE_2, lift), Stroke::new(1.0, theme::BORDER), StrokeKind::Inside));
        let angle = paint::spring(hover) * std::f32::consts::FRAC_PI_2;
        let icon = Rect::from_center_size(r.center(), Vec2::splat(18.0));
        egui::Image::new(icons::source_of(Icon::Gear))
            .fit_to_exact_size(icon.size())
            .tint(theme::lerp(theme::TEXT_2, theme::TEXT, lift))
            .rotate(angle, Vec2::splat(0.5))
            .paint_at(ui, icon);
        resp.clicked()
    }

    // ---------- Zone principale ----------

    pub fn main_area(&mut self, ui: &mut Ui, rect: Rect, accent: Accent) {
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
        egui::ScrollArea::vertical().id_salt("main").auto_shrink([false, false]).show(&mut child, |ui| {
            let width = (rect.width() - 56.0).max(200.0);
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.add_space(28.0);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    if self.service.available == Some(false) {
                        self.service_banner(ui);
                        ui.add_space(16.0);
                    }
                    if self.tunnels.is_empty() {
                        let h = (rect.height() - 46.0).max(420.0);
                        self.empty_view(ui, width, h, accent);
                    } else if self.current().is_some() {
                        self.tunnel_view(ui, width, accent);
                    }
                });
            });
            ui.add_space(28.0);
            if self.scroll_to_bottom {
                ui.scroll_to_cursor(Some(Align::BOTTOM));
            }
        });
    }

    fn service_banner(&self, ui: &mut Ui) {
        let width = ui.available_width();
        let mut sub = theme::job(
            self.service.error.clone().unwrap_or_else(|| "Vous pouvez gérer vos configurations, mais la connexion nécessite le service.".into()),
            13.0,
            Weight::Regular,
            Color32::from_rgb(0xe9, 0xd5, 0xa6),
        );
        sub.wrap.max_width = width - 16.0 * 2.0 - 32.0;
        let title = ui.painter().layout_job(theme::job("Service Cryptonaute injoignable", 14.0, Weight::Bold, theme::WARN_TEXT));
        let sub = ui.painter().layout_job(sub);
        let h = 12.0 * 2.0 + title.size().y + 2.0 + sub.size().y;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
        ui.painter().add(RectShape::new(rect, cr(14.0), theme::rgba(245, 158, 11, 0.1), Stroke::new(1.0, theme::rgba(245, 158, 11, 0.3)), StrokeKind::Inside));
        icons::paint(ui, Icon::Warn, Rect::from_min_size(rect.min + Vec2::new(16.0, 13.0), Vec2::splat(20.0)), theme::WARN_TEXT);
        let x = rect.left() + 16.0 + 32.0;
        ui.painter().galley(Pos2::new(x, rect.top() + 12.0), title.clone(), theme::WARN_TEXT);
        ui.painter().galley(Pos2::new(x, rect.top() + 12.0 + title.size().y + 2.0), sub, theme::TEXT_2);
    }

    /// Écran d'accueil : aucun tunnel enregistré.
    fn empty_view(&mut self, ui: &mut Ui, width: f32, height: f32, accent: Accent) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let time = ui.input(|i| i.time);
        let block_h = 180.0 + 22.0 + 40.0 + 4.0 + 46.0 + 22.0 + 46.0;
        let top = rect.center().y - block_h / 2.0;
        let c = Pos2::new(rect.center().x, top + 90.0);

        // Orbites animées et bouclier flottant
        for (r, period, reverse, alpha) in [(60.0, 6.0, false, 1.0), (80.0, 10.0, true, 0.7), (100.0, 16.0, false, 0.4)] {
            ui.painter().circle_stroke(c, r, Stroke::new(1.0, theme::fade(accent.a2, 0.4 * alpha)));
            let mut angle = (time / period).fract() as f32 * std::f32::consts::TAU;
            if reverse {
                angle = -angle;
            }
            let dot = c + Vec2::angled(angle - std::f32::consts::FRAC_PI_2) * r;
            ui.painter().add(paint::soft_disc(dot, 4.0, 12.0, theme::fade(accent.a2, alpha), true));
            ui.painter().circle_filled(dot, 4.0, theme::fade(accent.a2, alpha));
        }
        let bob = -6.0 * paint::ping_pong(time, 4.0);
        let shield = Rect::from_center_size(c + Vec2::new(0.0, bob), Vec2::splat(64.0));
        ui.painter().add(paint::soft_disc(shield.center(), 20.0, 24.0, theme::fade(accent.a1, 0.8), true));
        icons::paint_svg(ui, "bytes://shield.svg", icons::SHIELD_SVG, shield, accent.a1);

        // Titre en dégradé (blanc → accent)
        let mut y = top + 180.0 + 22.0;
        let mut title = theme::job("Bienvenue dans Cryptonaute", 30.0, Weight::Bold, Color32::WHITE);
        title.sections[0].format.extra_letter_spacing = -0.5;
        let g = ui.painter().layout_job(title);
        let pos = Pos2::new(rect.center().x - g.size().x / 2.0, y);
        let grect = Rect::from_min_size(pos, g.size());
        let brush = paint::linear(grect, 120.0, vec![(0.0, Color32::WHITE), (0.3, Color32::WHITE), (1.0, accent.a2)]);
        paint::gradient_text(ui.painter(), pos, g.clone(), &brush);
        y += g.size().y + 8.0;

        // Texte d'aide, avec `.conf` en code
        let mut job = egui::text::LayoutJob::default();
        let base = |c| {
            let mut f = theme::format(14.0, Weight::Regular, c);
            f.line_height = Some(14.0 * 1.6);
            f
        };
        job.append("Glissez-déposez un fichier ", 0.0, base(theme::TEXT_2));
        let mut code = theme::mono(12.0, theme::TEXT);
        code.background = theme::SURFACE_2;
        code.line_height = Some(14.0 * 1.6);
        job.append(" .conf ", 0.0, code);
        job.append(" WireGuard ici, ou créez un tunnel.\nAucun droit administrateur n'est nécessaire.", 0.0, base(theme::TEXT_2));
        job.halign = Align::Center;
        let g = ui.painter().layout_job(job);
        ui.painter().galley(Pos2::new(rect.center().x, y), g.clone(), theme::TEXT_2);
        y += g.size().y + 22.0;

        // Actions
        let w1 = 252.0;
        let w2 = 214.0;
        let x = rect.center().x - (w1 + 12.0 + w2) / 2.0;
        let mut row = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(x, y), Vec2::new(w1 + 12.0 + w2, 46.0))).layout(Layout::left_to_right(Align::Center)));
        row.spacing_mut().item_spacing.x = 12.0;
        if Button::new("Importer une configuration").icon(Icon::Import).kind(Kind::Primary).size(Size::Large).width(w1).show(&mut row, accent).clicked() {
            self.pick_import_files();
        }
        if Button::new("Créer à partir de zéro").kind(Kind::Ghost).size(Size::Large).width(w2).show(&mut row, accent).clicked() {
            self.new_tunnel();
        }
    }

    /// Détail du tunnel sélectionné.
    fn tunnel_view(&mut self, ui: &mut Ui, width: f32, accent: Accent) {
        let Some(t) = self.current().cloned() else { return };
        // Entrée animée de la vue (`viewIn`)
        let p = paint::ease_out(((self.now - self.view_since) / 0.5).clamp(0.0, 1.0) as f32);
        ui.set_opacity(p);
        let offset = 10.0 * (1.0 - p);
        ui.add_space(offset);

        // En-tête : nom, puces et actions
        let top = ui.cursor().min;
        let hero_h = 18.0 + 4.0 + 40.0 + 10.0 + 26.0;
        let (hero, _) = ui.allocate_exact_size(Vec2::new(width, hero_h), Sense::hover());
        let eyebrow = ui.painter().layout_job(theme::caps("Tunnel WireGuard", 12.0, 1.8, theme::MUTED));
        ui.painter().galley(hero.min, eyebrow.clone(), theme::MUTED);
        let mut name_job = theme::job(t.name.clone(), 32.0, Weight::Bold, theme::TEXT);
        name_job.sections[0].format.extra_letter_spacing = -0.6;
        let name = ui.painter().layout_job(name_job);
        let name_y = hero.top() + eyebrow.size().y + 4.0;
        ui.painter().galley(Pos2::new(hero.left(), name_y), name.clone(), theme::TEXT);
        let mut chips = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(hero.left(), name_y + name.size().y + 10.0), Vec2::new(width - 120.0, 26.0))).layout(Layout::left_to_right(Align::Center)));
        chips.spacing_mut().item_spacing.x = 8.0;
        match (&t.error, &t.summary) {
            (Some(e), _) => {
                widgets::chip(&mut chips, Icon::Warn, e, ChipKind::Warn, accent);
            }
            (None, Some(s)) => {
                if s.full_tunnel {
                    widgets::chip(&mut chips, Icon::Globe, "Tout le trafic", ChipKind::Accent, accent);
                } else {
                    widgets::chip(&mut chips, Icon::Split, "Tunnel partiel", ChipKind::Normal, accent);
                }
                let n = s.peers.len();
                widgets::chip(&mut chips, Icon::Server, &format!("{n} pair{}", if n > 1 { "s" } else { "" }), ChipKind::Normal, accent);
                if !s.warnings.is_empty() {
                    let w = s.warnings.len();
                    widgets::chip(&mut chips, Icon::Warn, &format!("{w} avertissement{}", if w > 1 { "s" } else { "" }), ChipKind::Warn, accent)
                        .on_hover_text(s.warnings.join("\n"));
                }
            }
            _ => {}
        }
        let mut actions = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(hero.right() - 88.0, hero.bottom() - 40.0), hero.max)).layout(Layout::left_to_right(Align::Center)));
        actions.spacing_mut().item_spacing.x = 8.0;
        if widgets::icon_button(&mut actions, Icon::Edit, false, false, "Modifier").clicked() {
            self.edit_tunnel();
        }
        if widgets::icon_button(&mut actions, Icon::Trash, false, true, "Supprimer").clicked() {
            self.ask_delete();
        }
        let _ = top;
        ui.add_space(18.0);

        self.connect_card(ui, width, &t, accent);
        ui.add_space(16.0);
        self.stats(ui, width, &t, accent);
        ui.add_space(16.0);
        chart::chart_card(self, ui, width, &t.name);
        ui.add_space(16.0);
        self.details(ui, width, &t);
    }

    /// Carte de connexion : bouton, état et informations rapides.
    fn connect_card(&mut self, ui: &mut Ui, width: f32, t: &crate::backend::TunnelEntry, accent: Accent) {
        let narrow = ui.ctx().content_rect().width() < 1080.0;
        let height = if narrow { 26.0 + 132.0 + 16.0 + 1.0 + 16.0 + 40.0 + 26.0 } else { 26.0 * 2.0 + 132.0 };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let ctx = ui.ctx().clone();
        ui.painter().extend(paint::glass(&ctx, rect, 18.0));
        // Lueur d'accent à gauche (`radial-gradient(500px 200px at 0% 50%, ...)`)
        let glow = paint::radial(Pos2::new(rect.left(), rect.center().y), Vec2::new(500.0, 200.0), vec![(0.0, theme::fade(accent.a1, 0.22)), (0.7, Color32::TRANSPARENT)]);
        ui.painter().add(paint::gradient_rect(&ctx, rect.shrink(1.0), 17.0, &glow));

        let mood = self.mood();
        let power_rect = Rect::from_min_size(rect.min + Vec2::new(30.0, 26.0), Vec2::splat(132.0));
        let enabled = t.error.is_none() && self.pending.is_none() && self.service.available != Some(false);
        let active = self.is_active(&t.name);
        if power::power_button(ui, power_rect, mood, enabled, accent, if active { "Se déconnecter" } else { "Se connecter" }).clicked() {
            self.toggle_power();
        }

        // État
        let (label, sub, warn) = self.state_texts(t);
        let quick_w = 220.0 + 24.0;
        let state_left = power_rect.right() + 28.0;
        let state_right = if narrow { rect.right() - 30.0 } else { rect.right() - 30.0 - quick_w - 28.0 };
        let state_w = (state_right - state_left).max(120.0);
        let mut label_job = theme::job(label, 26.0, Weight::Bold, theme::TEXT);
        label_job.sections[0].format.extra_letter_spacing = -0.4;
        let lg = ui.painter().layout_job(label_job);
        let mut sub_job = theme::job(sub, 14.0, Weight::Regular, if warn { theme::WARN_TEXT } else { theme::TEXT_2 });
        sub_job.sections[0].format.line_height = Some(21.0);
        sub_job.wrap.max_width = state_w;
        let sg = ui.painter().layout_job(sub_job);
        let st = self.status_of(&t.name).cloned();
        let timer = match &st {
            Some(s) => format::duration(self.now - s.connected_since as f64),
            None => "00:00:00".into(),
        };
        let mut timer_job = egui::text::LayoutJob::single_section(timer, theme::mono(15.0, if mood == Mood::Connected { theme::TEXT } else { theme::MUTED }));
        timer_job.sections[0].format.extra_letter_spacing = 1.0;
        let tg = ui.painter().layout_job(timer_job);
        let block = lg.size().y + 4.0 + sg.size().y + 10.0 + tg.size().y;
        let mut y = power_rect.center().y - block / 2.0;
        let lpos = Pos2::new(state_left, y);
        match mood {
            Mood::Connected => {
                let r = Rect::from_min_size(lpos, lg.size());
                let brush = paint::linear(r, 120.0, vec![(0.0, Color32::from_rgb(0xa7, 0xf3, 0xd0)), (1.0, accent.a2)]);
                paint::gradient_text(ui.painter(), lpos, lg.clone(), &brush);
            }
            Mood::Error => ui.painter().galley_with_override_text_color(lpos, lg.clone(), theme::ERROR_TEXT),
            _ => ui.painter().galley(lpos, lg.clone(), theme::TEXT),
        }
        y += lg.size().y + 4.0;
        ui.painter().galley(Pos2::new(state_left, y), sg.clone(), theme::TEXT_2);
        y += sg.size().y + 10.0;
        ui.painter().galley(Pos2::new(state_left, y), tg, theme::MUTED);

        // Informations rapides
        let summary = t.summary.as_ref();
        let endpoint = st
            .as_ref()
            .and_then(|s| s.endpoint.clone())
            .or_else(|| summary.and_then(|s| s.peers.first()).and_then(|p| p.endpoint.clone()))
            .unwrap_or_else(|| "—".into());
        let address = summary.map(|s| s.addresses.join(", ")).filter(|a| !a.is_empty()).unwrap_or_else(|| "—".into());
        let handshake = match &st {
            Some(s) => match s.last_handshake {
                Some(h) => format::ago((self.now as u64).saturating_sub(h)),
                None => "en attente…".into(),
            },
            None => "—".into(),
        };
        let items = [("Endpoint", endpoint, true), ("Adresse", address, true), ("Handshake", handshake, false)];
        if narrow {
            let top = power_rect.bottom() + 16.0;
            ui.painter().hline(rect.left() + 30.0..=rect.right() - 30.0, top, Stroke::new(1.0, theme::BORDER));
            let col = (rect.width() - 60.0) / 3.0;
            for (i, (k, v, mono)) in items.into_iter().enumerate() {
                quick_item(ui, Pos2::new(rect.left() + 30.0 + col * i as f32, top + 17.0), col - 12.0, k, &v, mono);
            }
        } else {
            let left = rect.right() - 30.0 - quick_w;
            ui.painter().vline(left, power_rect.y_range(), Stroke::new(1.0, theme::BORDER));
            let mut y = power_rect.center().y - (3.0 * 34.0 + 2.0 * 10.0) / 2.0;
            for (k, v, mono) in items {
                quick_item(ui, Pos2::new(left + 24.0, y), 220.0, k, &v, mono);
                y += 34.0 + 10.0;
            }
        }
    }

    /// Libellé, sous-titre (et s'il s'agit d'un avertissement) de l'état du tunnel.
    fn state_texts(&self, t: &crate::backend::TunnelEntry) -> (String, String, bool) {
        let st = self.status_of(&t.name);
        let others: Vec<&cryptonaute_common::ipc::TunnelStatus> = self.active_list().iter().filter(|s| s.name != t.name).collect();
        let other_full = others.iter().find(|s| s.full_tunnel).map(|s| s.name.clone());
        let others_split: Vec<String> = others.iter().filter(|s| !s.full_tunnel).map(|s| s.name.clone()).collect();
        let full = t.summary.as_ref().is_some_and(|s| s.full_tunnel);
        match self.mood() {
            Mood::Connecting => ("Connexion…".into(), "Création de l’interface et négociation avec le serveur".into(), false),
            Mood::Disconnecting => ("Déconnexion…".into(), "Fermeture du tunnel".into(), false),
            Mood::Connected => {
                let st = st.cloned();
                let since = st.as_ref().map(|s| self.now - s.connected_since as f64).unwrap_or(0.0);
                let hs = st.as_ref().and_then(|s| s.last_handshake);
                let sub = if hs.is_none() && since > 8.0 {
                    return ("Connecté".into(), "En attente de la poignée de main… vérifiez l’endpoint et les clés".into(), true);
                } else if hs.is_some_and(|h| self.now - h as f64 > 180.0) {
                    return ("Connecté".into(), "Le serveur ne répond plus depuis quelques minutes".into(), true);
                } else if !full && other_full.is_some() {
                    format!("Ses réseaux passent par ce tunnel, le reste du trafic par « {} »", other_full.unwrap_or_default())
                } else if full && !others_split.is_empty() {
                    format!("Tout le trafic passe par ce tunnel, sauf les réseaux de {}", super::app::quote_list(&others_split))
                } else {
                    format!("Trafic chiffré via {}", st.and_then(|s| s.endpoint).unwrap_or_else(|| "le serveur".into()))
                };
                ("Connecté".into(), sub, false)
            }
            Mood::Error => (
                "Échec de connexion".into(),
                self.last_error.as_ref().map(|(_, m)| m.clone()).unwrap_or_default(),
                false,
            ),
            Mood::Disconnected => {
                let sub = if self.service.available == Some(false) {
                    "Le service Cryptonaute est requis pour se connecter".to_string()
                } else if t.error.is_some() || others.is_empty() {
                    "Cliquez sur le bouton pour vous connecter".to_string()
                } else if full && other_full.is_some() {
                    format!("Remplacera « {} » : un seul tunnel complet à la fois", other_full.unwrap_or_default())
                } else if full {
                    format!("S’ajoutera à {}, qui gardera ses réseaux", super::app::quote_list(&others_split))
                } else if let Some(f) = other_full {
                    format!("S’ajoutera à « {f} » : ses réseaux passeront par ce tunnel")
                } else {
                    let names: Vec<String> = others.iter().map(|s| s.name.clone()).collect();
                    format!("S’ajoutera à {}", super::app::quote_list(&names))
                };
                ("Déconnecté".into(), sub, false)
            }
        }
    }

    /// Statistiques : reçu, envoyé, routage.
    fn stats(&mut self, ui: &mut Ui, width: f32, t: &crate::backend::TunnelEntry, accent: Accent) {
        let _ = accent;
        let col = (width - 32.0) / 3.0;
        let st = self.status_of(&t.name).cloned();
        let (rx_rate, tx_rate) = if st.is_some() { self.last_rate(&t.name) } else { (0.0, 0.0) };
        let rx = format::bytes(st.as_ref().map_or(0.0, |s| s.rx_bytes as f64));
        let tx = format::bytes(st.as_ref().map_or(0.0, |s| s.tx_bytes as f64));
        let (mode, mode_sub) = match &t.summary {
            Some(s) if s.full_tunnel => ("Complet".to_string(), "Tout le trafic passe par le VPN".to_string()),
            Some(s) => ("Partiel".to_string(), format!("{} réseau(x) routé(s)", s.peers.iter().map(|p| p.allowed_ips.len()).sum::<usize>())),
            None => ("—".to_string(), "—".to_string()),
        };
        let cards = [
            ("rx", Icon::ArrowDown, theme::rgba(34, 211, 238, 0.14), theme::CYAN, "Reçu", rx, format::rate(rx_rate)),
            ("tx", Icon::ArrowUp, theme::rgba(124, 92, 255, 0.18), theme::VIOLET_LIGHT, "Envoyé", tx, format::rate(tx_rate)),
            ("mode", Icon::Globe, theme::rgba(16, 185, 129, 0.14), theme::TEAL, "Routage", mode, mode_sub),
        ];
        let height = 96.0;
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        for (i, (key, icon, bg, fg, label, value, sub)) in cards.into_iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(row.left() + (col + 16.0) * i as f32, row.top()), Vec2::new(col, height));
            let resp = ui.interact(r, Id::new(("stat", key)), Sense::hover());
            let lift = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.3);
            let r = r.translate(Vec2::new(0.0, -3.0 * paint::spring(lift)));
            ui.painter().extend(paint::glass(ui.ctx(), r, 18.0));
            let icon_rect = Rect::from_min_size(Pos2::new(r.left() + 18.0, r.center().y - 22.0), Vec2::splat(44.0));
            ui.painter().rect_filled(icon_rect, cr(14.0), bg);
            icons::paint_centered(ui, icon, icon_rect.center(), 20.0, fg);
            let x = icon_rect.right() + 14.0;
            let text_w = r.right() - 18.0 - x;
            let lg = ui.painter().layout_job(theme::caps(label, 12.0, 1.2, theme::MUTED));
            let pulse = self.pulse(key, &value);
            let mut vj = theme::job(value, 22.0, Weight::Bold, theme::TEXT);
            vj.sections[0].format.extra_letter_spacing = -0.3;
            let vg = ui.painter().layout_job(vj);
            let mut sj = theme::job(sub, 12.0, Weight::Regular, theme::TEXT_2);
            sj.wrap.max_width = text_w;
            let sg = ui.painter().layout_job(sj);
            let block = lg.size().y + 2.0 + vg.size().y + 2.0 + sg.size().y;
            let mut y = r.center().y - block / 2.0;
            ui.painter().galley(Pos2::new(x, y), lg.clone(), theme::MUTED);
            y += lg.size().y + 2.0;
            let vpos = Pos2::new(x, y);
            ui.painter().galley_with_override_text_color(vpos, vg.clone(), theme::lerp(theme::TEXT, Color32::WHITE, pulse));
            y += vg.size().y + 2.0;
            ui.painter().galley(Pos2::new(x, y), sg, theme::TEXT_2);
        }
    }

    /// Effet de pulsation quand une valeur change (`.pulse`) : 1 → 0 en 0,8 s.
    fn pulse(&mut self, key: &'static str, value: &str) -> f32 {
        let now = self.now;
        let entry = self.pulses.entry(key).or_insert_with(|| (value.to_string(), now - 1.0));
        if entry.0 != value {
            *entry = (value.to_string(), now);
        }
        (1.0 - ((now - entry.1) / 0.8) as f32).clamp(0.0, 1.0)
    }

    /// Cartes « Interface » et « Pairs ».
    fn details(&mut self, ui: &mut Ui, width: f32, t: &crate::backend::TunnelEntry) {
        let Some(s) = t.summary.clone() else { return };
        let narrow = ui.ctx().content_rect().width() < 1080.0;
        let col = if narrow { width } else { (width - 16.0) / 2.0 };
        let start = ui.cursor().min;
        let mut copied: Option<String> = None;

        let mut interface = |ui: &mut Ui| {
            card_title(ui, "Interface");
            let mut rows: Vec<(&str, Row)> = vec![
                ("Clé publique", Row::Copy(s.public_key.clone())),
                ("Adresses", Row::Tags(s.addresses.clone())),
                ("DNS", Row::Tags(s.dns.clone())),
                ("MTU", Row::Tags(vec![s.mtu.map(|m| m.to_string()).unwrap_or_else(|| "auto (1420)".into())])),
            ];
            if let Some(p) = s.listen_port {
                rows.push(("Port d’écoute", Row::Tags(vec![p.to_string()])));
            }
            definition_list(ui, &rows, &mut copied);
        };
        let left = Rect::from_min_size(start, Vec2::new(col, 10_000.0));
        let mut lui = ui.new_child(UiBuilder::new().max_rect(left));
        let (lrect, _) = widgets::glass_card(&mut lui, Vec2::new(20.0, 18.0), 18.0, &mut interface);

        let peers_pos = if narrow { Pos2::new(start.x, lrect.bottom() + 16.0) } else { Pos2::new(start.x + col + 16.0, start.y) };
        let mut rui = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(peers_pos, Vec2::new(col, 10_000.0))));
        let (rrect, _) = widgets::glass_card(&mut rui, Vec2::new(20.0, 18.0), 18.0, |ui| {
            card_title(ui, "Pairs");
            for (i, p) in s.peers.iter().enumerate() {
                if i > 0 {
                    let y = ui.cursor().min.y + 12.0;
                    let r = ui.available_rect_before_wrap();
                    ui.painter().add(Shape::dashed_line(&[Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(1.0, theme::BORDER), 4.0, 3.0));
                    ui.add_space(25.0);
                }
                let mut rows: Vec<(&str, Row)> = vec![
                    ("Clé publique", Row::Copy(p.public_key.clone())),
                    ("Endpoint", Row::Tags(vec![p.endpoint.clone().unwrap_or_else(|| "—".into())])),
                    ("IP autorisées", Row::Tags(p.allowed_ips.clone())),
                    ("Keepalive", Row::Tags(vec![p.persistent_keepalive.map(|k| format!("{k} s")).unwrap_or_else(|| "désactivé".into())])),
                ];
                if p.has_preshared_key {
                    rows.push(("Clé pré-partagée", Row::Tags(vec!["activée".into()])));
                }
                definition_list(ui, &rows, &mut copied);
            }
        });
        let bottom = lrect.bottom().max(rrect.bottom());
        ui.allocate_rect(Rect::from_min_max(start, Pos2::new(start.x + width, bottom)), Sense::hover());
        if let Some(text) = copied {
            ui.ctx().copy_text(text);
            self.toast_for("Copié dans le presse-papiers", super::app::ToastKind::Success, 1.8);
        }
    }
}

fn quick_item(ui: &Ui, pos: Pos2, width: f32, label: &str, value: &str, mono: bool) {
    let lg = ui.painter().layout_job(theme::caps(label, 11.0, 1.2, theme::MUTED));
    let mut vj = if mono {
        egui::text::LayoutJob::single_section(value.to_string(), theme::mono(13.0, theme::TEXT))
    } else {
        theme::job(value, 13.0, Weight::Regular, theme::TEXT)
    };
    vj.wrap = egui::text::TextWrapping::truncate_at_width(width.min(260.0));
    let vg = ui.painter().layout_job(vj);
    ui.painter().galley(pos, lg.clone(), theme::MUTED);
    ui.painter().galley(pos + Vec2::new(0.0, lg.size().y + 2.0), vg, theme::TEXT);
}

fn card_title(ui: &mut Ui, title: &str) {
    widgets::text(ui, theme::caps(title, 13.0, 1.4, theme::MUTED));
    ui.add_space(12.0);
}

enum Row {
    Tags(Vec<String>),
    Copy(String),
}

/// Liste de définitions (`<dl>`) : libellé à gauche, valeurs à droite.
fn definition_list(ui: &mut Ui, rows: &[(&str, Row)], copied: &mut Option<String>) {
    let label_w = rows
        .iter()
        .map(|(k, _)| ui.painter().layout_job(theme::job(*k, 13.0, Weight::Regular, theme::MUTED)).size().x)
        .fold(0.0, f32::max);
    for (i, (k, row)) in rows.iter().enumerate() {
        if i > 0 {
            ui.add_space(10.0);
        }
        ui.horizontal_top(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(label_w + 16.0, 20.0), Sense::hover());
            let g = ui.painter().layout_job(theme::job(*k, 13.0, Weight::Regular, theme::MUTED));
            ui.painter().galley(Pos2::new(r.left(), r.center().y - g.size().y / 2.0), g, theme::MUTED);
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    match row {
                        Row::Tags(values) => {
                            if values.is_empty() {
                                widgets::tag(ui, "—", theme::SURFACE_2, theme::TEXT);
                            }
                            for v in values {
                                widgets::tag(ui, v, theme::SURFACE_2, theme::TEXT);
                            }
                        }
                        Row::Copy(value) => {
                            // Clé longue : coupée sur plusieurs lignes, comme `word-break: break-all`.
                            let width = ui.available_width() - 30.0;
                            let mut job = egui::text::LayoutJob::single_section(value.clone(), theme::mono(12.0, theme::TEXT));
                            job.wrap.max_width = width - 16.0;
                            job.wrap.break_anywhere = true;
                            let g = ui.painter().layout_job(job);
                            let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(16.0, 4.0), Sense::hover());
                            ui.painter().rect_filled(r, cr(6.0), theme::SURFACE_2);
                            ui.painter().galley(r.min + Vec2::new(8.0, 2.0), g, theme::TEXT);
                            if widgets::copy_button(ui).clicked() {
                                *copied = Some(value.clone());
                            }
                        }
                    }
                });
            });
        });
    }
}

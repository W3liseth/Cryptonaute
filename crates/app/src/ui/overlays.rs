//! Calques au-dessus de l'interface : notifications (toasts) et voile de
//! glisser-déposer.

use eframe::egui::{self, Color32, Id, LayerId, Order, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};
use eframe::epaint::RectShape;

use super::app::{App, ToastKind};
use super::icons::{self, Icon};
use super::paint::{self, cr, WithOpacity};
use super::theme::{self, Accent, Weight};

/// Notifications en bas à droite : entrée « ressort », sortie en glissant.
pub fn toasts(app: &mut App, ctx: &egui::Context) {
    let now = app.now;
    app.toasts.retain(|t| now < t.created + t.duration + 0.3);
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("toasts")));
    let mut bottom = screen.bottom() - 20.0;
    for t in app.toasts.iter().rev() {
        let age = now - t.created;
        let (opacity, dx, scale) = if age > t.duration {
            let p = paint::ease_out(((age - t.duration) / 0.3) as f32);
            (1.0 - p, 40.0 * p, 1.0)
        } else {
            let p = (age / 0.45).clamp(0.0, 1.0) as f32;
            (paint::ease_out(p * 1.6), 40.0 * (1.0 - paint::spring(p)), 0.95 + 0.05 * paint::spring(p))
        };
        let mut job = theme::job(t.message.clone(), 13.0, Weight::Regular, theme::TEXT);
        job.sections[0].format.line_height = Some(13.0 * 1.45);
        job.wrap.max_width = 420.0 - 32.0 - 32.0;
        let galley = painter.layout_job(job);
        let width = (galley.size().x + 32.0 + 32.0).clamp(260.0, 420.0);
        let height = galley.size().y.max(22.0) + 24.0;
        let rect = Rect::from_min_max(Pos2::new(screen.right() - 20.0 - width, bottom - height), Pos2::new(screen.right() - 20.0, bottom));
        let rect = Rect::from_center_size(rect.center() + Vec2::new(dx, 0.0), rect.size() * scale);
        let p = painter.clone().with_opacity(opacity.clamp(0.0, 1.0));
        p.add(paint::shadow(rect, 14.0, Vec2::new(0.0, 20.0), 60.0, -20.0, theme::rgba(0, 0, 0, 0.6)));
        p.add(RectShape::new(rect, cr(14.0), theme::rgba(22, 24, 44, 0.95), Stroke::new(1.0, theme::BORDER_STRONG), StrokeKind::Inside));
        let (icon, bg, fg) = match t.kind {
            ToastKind::Success => (Icon::Check, theme::rgba(16, 185, 129, 0.2), Color32::from_rgb(0x34, 0xd3, 0x99)),
            ToastKind::Error => (Icon::Close, theme::rgba(244, 63, 94, 0.2), Color32::from_rgb(0xfb, 0x71, 0x85)),
            ToastKind::Info => (Icon::Info, theme::rgba(34, 211, 238, 0.18), theme::CYAN),
        };
        let icon_rect = Rect::from_min_size(Pos2::new(rect.left() + 16.0, rect.center().y - 11.0), Vec2::splat(22.0));
        p.rect_filled(icon_rect, cr(7.0), bg);
        let galley_pos = Pos2::new(icon_rect.right() + 10.0, rect.center().y - galley.size().y / 2.0);
        p.galley(galley_pos, galley, theme::TEXT);
        // Les icônes passent par un `Ui` : on dessine sur le même calque.
        let ui = egui::Ui::new(ctx.clone(), Id::new(("toast", t.id)), egui::UiBuilder::new().layer_id(p.layer_id()).max_rect(rect));
        let ui = {
            let mut ui = ui;
            ui.set_opacity(opacity.clamp(0.0, 1.0));
            ui
        };
        icons::paint_centered(&ui, icon, icon_rect.center(), 14.0, fg);
        bottom = rect.top() - 10.0;
    }
}

/// Voile affiché pendant qu'on survole la fenêtre avec des fichiers.
pub fn drop_overlay(app: &App, ctx: &egui::Context, accent: Accent) {
    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
    let t = ctx.animate_bool_with_time(Id::new("drop-overlay"), hovering, 0.2);
    if t <= 0.0 {
        return;
    }
    let _ = app;
    let screen = ctx.content_rect();
    let layer = LayerId::new(Order::Foreground, Id::new("drop"));
    let painter = ctx.layer_painter(layer).with_opacity(t);
    painter.rect_filled(screen, 0.0, theme::rgba(5, 6, 14, 0.7));
    let scale = 0.96 + 0.04 * paint::spring(t);
    let box_rect = Rect::from_center_size(screen.center(), Vec2::new(460.0, 210.0) * scale);
    painter.rect_filled(box_rect, cr(26.0), theme::fade(accent.a1, 0.1));
    painter.add(dashed_rounded_rect(box_rect, 26.0, Stroke::new(2.0, theme::fade(accent.a2, 0.7))));
    let time = ctx.input(|i| i.time);
    let bob = -6.0 * paint::ping_pong(time, 1.4);
    let ui = egui::Ui::new(ctx.clone(), Id::new("drop-ui"), egui::UiBuilder::new().layer_id(layer).max_rect(screen));
    let mut ui = ui;
    ui.set_opacity(t);
    icons::paint_centered(&ui, Icon::Import, Pos2::new(box_rect.center().x, box_rect.top() + 70.0 + bob), 44.0, accent.a2);
    let title = painter.layout_job(theme::job("Déposez vos fichiers .conf", 20.0, Weight::Semibold, theme::TEXT));
    painter.galley(Pos2::new(box_rect.center().x - title.size().x / 2.0, box_rect.top() + 108.0), title, theme::TEXT);
    let sub = painter.layout_job(theme::job("Ils seront chiffrés et enregistrés dans votre profil", 14.0, Weight::Regular, theme::TEXT_2));
    painter.galley(Pos2::new(box_rect.center().x - sub.size().x / 2.0, box_rect.top() + 142.0), sub, theme::TEXT_2);
}

/// Contour en pointillés d'un rectangle arrondi.
fn dashed_rounded_rect(rect: Rect, radius: f32, stroke: Stroke) -> Shape {
    let mut points = Vec::new();
    let corners = [
        (Pos2::new(rect.right() - radius, rect.top() + radius), -90.0f32),
        (Pos2::new(rect.right() - radius, rect.bottom() - radius), 0.0),
        (Pos2::new(rect.left() + radius, rect.bottom() - radius), 90.0),
        (Pos2::new(rect.left() + radius, rect.top() + radius), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=8 {
            let a = (start + 90.0 * i as f32 / 8.0).to_radians();
            points.push(c + Vec2::angled(a) * radius);
        }
    }
    points.push(points[0]);
    Shape::Vec(Shape::dashed_line(&points, stroke, 8.0, 6.0))
}

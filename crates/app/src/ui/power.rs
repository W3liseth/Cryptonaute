//! Bouton de connexion (`.power`) : halo conique flou, ondes une fois connecté,
//! anneau tournant pendant la connexion et cœur en dégradé radial.

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};
use eframe::epaint::Mesh;

use super::icons::{self, Icon};
use super::paint::{self, WithOpacity};
use super::theme::{self, Accent, Mood};

/// Disque flou dont la couleur dépend de l'angle (`conic-gradient` + `blur`).
fn conic_glow(center: Pos2, radius: f32, blur: f32, color: impl Fn(f32) -> Color32) -> Shape {
    const SEGMENTS: usize = 72;
    let rings: [(f32, f32); 6] = [
        (0.0, 1.0),
        ((radius - blur).max(0.0), 1.0),
        (radius, 0.5),
        (radius + blur * 0.5, 0.14),
        (radius + blur * 0.8, 0.03),
        (radius + blur, 0.0),
    ];
    let mut mesh = Mesh::default();
    let avg = color(0.0);
    mesh.colored_vertex(center, avg);
    for (i, &(r, a)) in rings.iter().enumerate().skip(1) {
        for s in 0..SEGMENTS {
            let angle = s as f32 / SEGMENTS as f32 * TAU;
            // 0° du dégradé conique en haut, sens horaire
            let c = color(angle).gamma_multiply(a);
            mesh.colored_vertex(center + Vec2::angled(angle - FRAC_PI_2) * r, c);
        }
        let ring = 1 + (i as u32 - 1) * SEGMENTS as u32;
        for s in 0..SEGMENTS as u32 {
            let next = (s + 1) % SEGMENTS as u32;
            if i == 1 {
                mesh.add_triangle(0, ring + s, ring + next);
            } else {
                let prev = ring - SEGMENTS as u32;
                mesh.add_triangle(prev + s, ring + s, ring + next);
                mesh.add_triangle(prev + s, ring + next, prev + next);
            }
        }
    }
    Shape::mesh(mesh)
}

/// Anneau dont l'opacité varie avec l'angle (anneau tournant de connexion).
fn spinner_ring(center: Pos2, radius: f32, width: f32, rotation: f32, accent: Accent) -> Shape {
    const SEGMENTS: usize = 96;
    let mut mesh = Mesh::default();
    for s in 0..=SEGMENTS {
        let t = s as f32 / SEGMENTS as f32;
        // transparent 0–55 %, accent à 85 %, blanc à 100 %
        let color = if t < 0.55 {
            Color32::TRANSPARENT
        } else if t < 0.85 {
            theme::fade(accent.a2, (t - 0.55) / 0.30)
        } else {
            theme::lerp(accent.a2, Color32::WHITE, (t - 0.85) / 0.15)
        };
        let angle = rotation + t * TAU - FRAC_PI_2;
        let dir = Vec2::angled(angle);
        mesh.colored_vertex(center + dir * (radius - width), color);
        mesh.colored_vertex(center + dir * radius, color);
        if s > 0 {
            let i = (s * 2) as u32;
            mesh.add_triangle(i - 2, i - 1, i);
            mesh.add_triangle(i - 1, i + 1, i);
        }
    }
    Shape::mesh(mesh)
}

pub fn power_button(ui: &mut Ui, rect: Rect, mood: Mood, enabled: bool, accent: Accent, tooltip: &str) -> Response {
    let id = ui.id().with("power");
    let response = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let ctx = ui.ctx().clone();
    let time = ui.input(|i| i.time);
    let painter = ui.painter();
    let c = rect.center();

    // Halo conique flou
    let halo_opacity = ctx.animate_value_with_time(
        id.with("halo"),
        match mood {
            Mood::Connected => 0.7,
            Mood::Connecting => 0.5,
            Mood::Error => 0.55,
            _ => 0.25,
        },
        0.6,
    );
    let breathe = if mood == Mood::Connecting { 1.0 + 0.08 * paint::ping_pong(time, 1.2) } else { 1.0 };
    let conic = |angle: f32| {
        let t = angle / TAU;
        let mix = if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 };
        theme::fade(theme::lerp(accent.a1, accent.a2, mix), halo_opacity)
    };
    painter.add(conic_glow(c, 66.0 * breathe - 7.0, 14.0, conic));

    // Ondes (connecté)
    if mood == Mood::Connected {
        for delay in [0.0, 1.3] {
            let phase = ((time + delay) / 2.6).fract() as f32;
            let e = paint::ease_out(phase);
            let scale = 0.85 + 0.7 * e;
            painter.circle_stroke(c, 52.0 * scale, Stroke::new(2.0, theme::fade(accent.a1, 0.8 * (1.0 - e))));
        }
    }

    // Anneau tournant (connexion / déconnexion)
    if matches!(mood, Mood::Connecting | Mood::Disconnecting) {
        let rotation = ((time / 0.9).fract() as f32) * TAU;
        painter.add(spinner_ring(c, 58.0, 4.0, rotation, accent));
    }

    // Cœur : survol (agrandi), clic (enfoncé)
    let hover = ctx.animate_bool_with_time(id.with("hover"), enabled && response.hovered(), 0.35);
    let pressed = enabled && response.is_pointer_button_down_on();
    let scale = if pressed { 0.94 } else { 1.0 + 0.05 * paint::spring(hover) };
    let r = 52.0 * scale;
    let connected = ctx.animate_bool_with_time(id.with("connected"), mood == Mood::Connected, 0.6);
    let opacity = if enabled { 1.0 } else { 0.55 };
    let core_painter = painter.clone().with_opacity(opacity);
    let core_rect = Rect::from_center_size(c, Vec2::splat(r * 2.0));
    core_painter.add(paint::shadow(core_rect, r, Vec2::new(0.0, 18.0), 40.0, -12.0, theme::rgba(0, 0, 0, 0.8 - 0.2 * connected)));
    if connected > 0.0 {
        core_painter.add(paint::shadow(core_rect, r, Vec2::ZERO, 50.0, -6.0, theme::fade(accent.a1, connected)));
    }
    let light = Pos2::new(c.x - r * 0.4, c.y - r * 0.5); // `at 30% 25%`
    let idle = paint::radial(light, Vec2::splat(r * 2.0), vec![(0.0, Color32::from_rgb(0x2b, 0x2f, 0x4d)), (0.7, Color32::from_rgb(0x13, 0x15, 0x2a))]);
    let on = paint::radial(light, Vec2::splat(r * 2.0), vec![(0.0, Color32::from_rgb(0x34, 0xd3, 0x99)), (0.75, Color32::from_rgb(0x05, 0x96, 0x69))]);
    let brush = move |p: Pos2| theme::lerp(idle(p), on(p), connected);
    core_painter.add(paint::gradient_circle(&ctx, c, r, &brush));
    core_painter.circle_stroke(c, r - 0.5, Stroke::new(1.0, theme::lerp(theme::BORDER_STRONG, theme::white_alpha(0.25), connected)));
    // Reflet intérieur en haut (`inset 0 2px 0`)
    let arc: Vec<Pos2> = (0..=24)
        .map(|i| {
            let a = -FRAC_PI_2 - 0.9 + 1.8 * i as f32 / 24.0;
            c + Vec2::angled(a) * (r - 2.0)
        })
        .collect();
    core_painter.add(Shape::line(arc, Stroke::new(2.0, theme::white_alpha(0.08 + 0.27 * connected))));

    // Icône
    let icon_color = theme::lerp(theme::lerp(theme::TEXT_2, Color32::WHITE, hover), Color32::WHITE, connected);
    if hover > 0.0 || connected > 0.0 {
        let glow = theme::lerp(accent.a2, Color32::WHITE, connected);
        core_painter.add(paint::soft_disc(c, 14.0, 12.0, theme::fade(glow, 0.25 * hover.max(connected)), true));
    }
    icons::paint_centered(ui, Icon::Power, c, 40.0 * scale, theme::fade(icon_color, opacity));

    let response = if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response.on_hover_cursor(CursorIcon::NotAllowed) };
    response.on_hover_text(tooltip)
}

#[allow(dead_code)]
fn _unused(_: egui::Context) {}

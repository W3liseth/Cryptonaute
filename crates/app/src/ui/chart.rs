//! Graphique du trafic en temps réel (60 dernières secondes), comme l'ancien
//! `<canvas>` : courbes lissées, remplissage en dégradé, halo et point final.

use eframe::egui::{self, text::LayoutJob, Align, Color32, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use eframe::epaint::Mesh;

use super::app::{App, Sample};
use super::format;
use super::theme::{self, Weight};
use super::widgets;

const WINDOW: f64 = 60.0;
const HEIGHT: f32 = 170.0;
const RX: Color32 = Color32::from_rgb(34, 211, 238);
const TX: Color32 = Color32::from_rgb(167, 139, 250);

pub fn chart_card(app: &mut App, ui: &mut Ui, width: f32, name: &str) {
    let samples: Vec<Sample> = app.samples_of(name).to_vec();
    let mut outer = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(ui.cursor().min, Vec2::new(width, 1000.0))));
    let (rect, _) = widgets::glass_card(&mut outer, Vec2::new(20.0, 12.0), 18.0, |ui| {
        ui.add_space(6.0);
        // En-tête : titre et légende
        let (head, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
        let title = ui.painter().layout_job(theme::caps("Trafic en temps réel", 13.0, 1.4, theme::MUTED));
        ui.painter().galley(Pos2::new(head.left(), head.center().y - title.size().y / 2.0), title, theme::MUTED);
        let mut x = head.right();
        for (label, color) in [("Émission", TX), ("Réception", RX)] {
            let g = ui.painter().layout_job(theme::job(label, 12.0, Weight::Regular, theme::TEXT_2));
            x -= g.size().x;
            ui.painter().galley(Pos2::new(x, head.center().y - g.size().y / 2.0), g, theme::TEXT_2);
            let sq = Rect::from_center_size(Pos2::new(x - 11.0, head.center().y), Vec2::splat(10.0));
            ui.painter().add(super::paint::shadow(sq, 3.0, Vec2::ZERO, 8.0, 0.0, theme::fade(color, 0.8)));
            ui.painter().rect_filled(sq, 3.0, color);
            x -= 16.0 + 16.0;
        }
        ui.add_space(8.0);
        let (canvas, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HEIGHT), Sense::hover());
        draw(ui, canvas, &samples);
    });
    ui.allocate_rect(rect, Sense::hover());
}

fn draw(ui: &Ui, rect: Rect, samples: &[Sample]) {
    let painter = ui.painter_at(rect);
    let now = format::now_secs() - 1.0; // léger retard : défilement fluide entre deux mesures
    let peak = samples.iter().map(|s| s.rx.max(s.tx)).fold(1024.0, f64::max);
    // Mise à l'échelle amortie (8 % par image)
    let max_id = Id::new("chart-max");
    let mut max = ui.ctx().memory(|m| m.data.get_temp::<f64>(max_id)).unwrap_or(1024.0);
    max += (peak * 1.25 - max) * 0.08;
    ui.ctx().memory_mut(|m| m.data.insert_temp(max_id, max));

    let (pt, pb, pl, pr) = (10.0, 20.0, 4.0, 56.0);
    let iw = rect.width() - pl - pr;
    let ih = rect.height() - pt - pb;
    let x = |ts: f64| rect.left() + pl + iw - ((now - ts) / WINDOW) as f32 * iw;
    let y = |v: f64| rect.top() + pt + ih - (v / max) as f32 * ih;

    // Grille et échelle
    let label_color = theme::rgba(184, 188, 217, 0.55);
    for i in 0..=3 {
        let v = max / 3.0 * i as f64;
        let yy = y(v).round() + 0.5;
        let stroke = Stroke::new(1.0, theme::white_alpha(0.06));
        let (a, b) = (Pos2::new(rect.left() + pl, yy), Pos2::new(rect.left() + pl + iw, yy));
        if i == 0 {
            painter.line_segment([a, b], stroke);
        } else {
            painter.add(Shape::dashed_line(&[a, b], stroke, 3.0, 5.0));
        }
        let g = painter.layout_job(theme::job(format::rate(v), 11.0, Weight::Regular, label_color));
        painter.galley(Pos2::new(rect.left() + pl + iw + 8.0, yy - g.size().y / 2.0), g, label_color);
    }
    let foot = theme::rgba(127, 132, 166, 0.6);
    let g = painter.layout_job(theme::job("-60 s", 11.0, Weight::Regular, foot));
    painter.galley(Pos2::new(rect.left() + pl, rect.bottom() - 8.0 - g.size().y / 2.0), g, foot);
    let mut job: LayoutJob = theme::job("maintenant", 11.0, Weight::Regular, foot);
    job.halign = Align::RIGHT;
    let g = painter.layout_job(job);
    painter.galley(Pos2::new(rect.left() + pl + iw, rect.bottom() - 8.0 - g.size().y / 2.0), g, foot);

    let visible: Vec<&Sample> = samples.iter().filter(|s| now - s.t <= WINDOW + 2.0).collect();
    if visible.len() < 2 {
        return;
    }
    let plot = painter.with_clip_rect(Rect::from_min_max(Pos2::new(rect.left() + pl, rect.top()), Pos2::new(rect.left() + pl + iw, rect.bottom())));
    let base = rect.top() + pt + ih;
    for (key, color) in [(0, TX), (1, RX)] {
        let value = |s: &Sample| if key == 0 { s.tx } else { s.rx };
        // Courbe de Bézier passant par les mesures (points de contrôle au milieu en x).
        let mut points: Vec<Pos2> = Vec::new();
        for (i, s) in visible.iter().enumerate() {
            let p = Pos2::new(x(s.t), y(value(s)));
            if i == 0 {
                points.push(p);
                continue;
            }
            let prev = visible[i - 1];
            let p0 = Pos2::new(x(prev.t), y(value(prev)));
            let cx = (p0.x + p.x) / 2.0;
            let (c1, c2) = (Pos2::new(cx, p0.y), Pos2::new(cx, p.y));
            for k in 1..=10 {
                let t = k as f32 / 10.0;
                let u = 1.0 - t;
                let q = p0.to_vec2() * u * u * u + c1.to_vec2() * 3.0 * u * u * t + c2.to_vec2() * 3.0 * u * t * t + p.to_vec2() * t * t * t;
                points.push(q.to_pos2());
            }
        }

        // Remplissage : dégradé vertical, 35 % d'opacité en haut du graphique,
        // transparent en bas.
        let shade = |py: f32| theme::fade(color, 0.35 * ((base - py) / ih).clamp(0.0, 1.0));
        let mut mesh = Mesh::default();
        for (i, p) in points.iter().enumerate() {
            mesh.colored_vertex(*p, shade(p.y));
            mesh.colored_vertex(Pos2::new(p.x, base), Color32::TRANSPARENT);
            if i > 0 {
                let j = (i * 2) as u32;
                mesh.add_triangle(j - 2, j - 1, j);
                mesh.add_triangle(j - 1, j + 1, j);
            }
        }
        plot.add(Shape::mesh(mesh));

        // Halo puis trait
        plot.add(Shape::line(points.clone(), Stroke::new(8.0, theme::fade(color, 0.10))));
        plot.add(Shape::line(points.clone(), Stroke::new(5.0, theme::fade(color, 0.18))));
        plot.add(Shape::line(points.clone(), Stroke::new(2.0, color)));
        if let Some(last) = points.last() {
            plot.circle_filled(*last, 3.5, color);
        }
    }
}

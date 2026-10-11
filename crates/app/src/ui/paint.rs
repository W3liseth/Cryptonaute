//! Primitives de dessin : dégradés, halos flous, cartes « verre » et fond animé.
//!
//! egui ne remplit les formes qu'avec une couleur unie. Pour les dégradés
//! (boutons, cartes, texte en dégradé…), la forme est tessellée en blanc puis
//! chaque sommet est recolorié selon sa position : l'antialiasing est conservé,
//! comme avec `background: linear-gradient(...)` ou `background-clip: text`.

use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};
use eframe::epaint::{Mesh, RectShape, Shadow, TessellationOptions, Tessellator};

use super::theme::{self, Accent};

/// `painter.with_opacity(x)` : copie du pinceau avec une opacité réduite.
pub trait WithOpacity {
    fn with_opacity(self, opacity: f32) -> Self;
}

impl WithOpacity for Painter {
    fn with_opacity(mut self, opacity: f32) -> Self {
        self.multiply_opacity(opacity);
        self
    }
}

/// Couleur en fonction de la position.
pub type Brush<'a> = &'a dyn Fn(Pos2) -> Color32;

/// Tessellation d'une forme, recoloriée par `brush` (la couverture d'antialiasing
/// de chaque sommet est conservée).
pub fn gradient_shape(ctx: &egui::Context, shape: Shape, brush: Brush<'_>) -> Shape {
    // Les disques précalculés ne sont qu'une optimisation : on s'en passe.
    let (tex_size, discs) = (ctx.fonts(|f| f.font_image_size()), Vec::new());
    let mut tess = Tessellator::new(ctx.pixels_per_point(), TessellationOptions::default(), tex_size, discs);
    let mut mesh = Mesh::default();
    tess.tessellate_shape(shape, &mut mesh);
    for v in &mut mesh.vertices {
        let coverage = v.color.a() as f32 / 255.0;
        v.color = brush(v.pos).gamma_multiply(coverage);
    }
    Shape::mesh(mesh)
}

/// Rectangle arrondi rempli d'un dégradé.
pub fn gradient_rect(ctx: &egui::Context, rect: Rect, radius: f32, brush: Brush<'_>) -> Shape {
    gradient_shape(ctx, Shape::rect_filled(rect, cr(radius), Color32::WHITE), brush)
}

/// Disque rempli d'un dégradé.
pub fn gradient_circle(ctx: &egui::Context, center: Pos2, radius: f32, brush: Brush<'_>) -> Shape {
    gradient_shape(ctx, Shape::circle_filled(center, radius, Color32::WHITE), brush)
}

pub fn cr(radius: f32) -> CornerRadius {
    CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8)
}

/// Interpole une liste d'arrêts de couleur `(position 0..1, couleur)`.
pub fn stops_at(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    match stops.iter().position(|(p, _)| *p >= t) {
        None => stops.last().map(|s| s.1).unwrap_or_default(),
        Some(0) => stops[0].1,
        Some(i) => {
            let (p0, c0) = stops[i - 1];
            let (p1, c1) = stops[i];
            theme::lerp(c0, c1, if p1 > p0 { (t - p0) / (p1 - p0) } else { 1.0 })
        }
    }
}

/// Dégradé linéaire CSS (`linear-gradient(<angle>deg, ...)`) sur `rect`.
pub fn linear(rect: Rect, angle_deg: f32, stops: Vec<(f32, Color32)>) -> impl Fn(Pos2) -> Color32 {
    let a = angle_deg.to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    let len = (rect.width() * a.sin()).abs() + (rect.height() * a.cos()).abs();
    let center = rect.center();
    move |p: Pos2| stops_at(&stops, (p - center).dot(dir) / len.max(1.0) + 0.5)
}

/// Dégradé radial elliptique (`radial-gradient(rx ry at center, ...)`).
pub fn radial(center: Pos2, radius: Vec2, stops: Vec<(f32, Color32)>) -> impl Fn(Pos2) -> Color32 {
    move |p: Pos2| {
        let d = (p - center) / radius;
        stops_at(&stops, d.length())
    }
}

/// Ombre portée CSS (`box-shadow: x y blur spread color`), étalement négatif compris.
pub fn shadow(rect: Rect, radius: f32, offset: Vec2, blur: f32, spread: f32, color: Color32) -> Shape {
    let rect = rect.expand(spread);
    let shadow = Shadow {
        offset: [offset.x.round() as i8, offset.y.round() as i8],
        blur: blur.round().clamp(0.0, 255.0) as u8,
        spread: 0,
        color,
    };
    Shape::Rect(shadow.as_shape(rect, cr((radius + spread).max(0.0))))
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Contour d'un rectangle arrondi : points et normales sortantes, coin par coin.
fn rounded_contour(rect: Rect, radius: f32, steps: usize) -> Vec<(Pos2, Vec2)> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let corners = [
        (Pos2::new(rect.right() - r, rect.top() + r), -90.0f32),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 90.0),
        (Pos2::new(rect.left() + r, rect.top() + r), 180.0),
    ];
    let mut out = Vec::with_capacity(4 * (steps + 1));
    for (c, start) in corners {
        for i in 0..=steps {
            let n = Vec2::angled((start + 90.0 * i as f32 / steps as f32).to_radians());
            out.push((c + n * r, n));
        }
    }
    out
}

/// Ombre portée dessinée uniquement à l'extérieur de `rect`, comme `box-shadow`
/// sous une carte translucide (egui assombrirait aussi l'intérieur).
pub fn outer_shadow(rect: Rect, radius: f32, offset: Vec2, blur: f32, spread: f32, color: Color32) -> Shape {
    let s_rect = rect.expand(spread).translate(offset);
    let s_radius = (radius + spread).max(0.0);
    let half = s_rect.size() / 2.0 - Vec2::splat(s_radius);
    let center = s_rect.center();
    let coverage = |p: Pos2| {
        let q = (p - center).abs() - half;
        let sd = Vec2::new(q.x.max(0.0), q.y.max(0.0)).length() + q.x.max(q.y).min(0.0) - s_radius;
        1.0 - smoothstep(-blur * 0.5, blur * 0.5, sd)
    };
    let reach = (blur * 0.5 + offset.length() + spread).max(4.0);
    let rings = [0.0, 0.08, 0.2, 0.38, 0.6, 0.8, 1.0].map(|f| f * reach);
    let contour = rounded_contour(rect, radius, 8);
    let n = contour.len() as u32;
    let mut mesh = Mesh::default();
    for d in rings {
        for (p, normal) in &contour {
            let pos = *p + *normal * d;
            mesh.colored_vertex(pos, color.gamma_multiply(coverage(pos)));
        }
    }
    for k in 1..rings.len() as u32 {
        let (inner, outer) = ((k - 1) * n, k * n);
        for i in 0..n {
            let j = (i + 1) % n;
            mesh.add_triangle(inner + i, outer + i, outer + j);
            mesh.add_triangle(inner + i, outer + j, inner + j);
        }
    }
    Shape::mesh(mesh)
}

/// Halo circulaire flou (cercle CSS avec `filter: blur()`), de taille quelconque.
/// `additive` reproduit `mix-blend-mode: screen`.
pub fn soft_disc(center: Pos2, radius: f32, blur: f32, color: Color32, additive: bool) -> Shape {
    // Profil d'un disque flouté par une gaussienne : plein au centre, puis décroissance
    // autour du bord, sur ±blur.
    let rings: [(f32, f32); 7] = [
        (0.0, 1.0),
        ((radius - blur).max(0.0), 1.0),
        ((radius - blur * 0.5).max(0.0), 0.86),
        (radius, 0.5),
        (radius + blur * 0.5, 0.14),
        (radius + blur * 0.8, 0.03),
        (radius + blur, 0.0),
    ];
    const SEGMENTS: usize = 72;
    let base = if additive {
        Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 0)
    } else {
        color
    };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, base);
    for (i, &(r, a)) in rings.iter().enumerate().skip(1) {
        for s in 0..SEGMENTS {
            let angle = s as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(center + Vec2::angled(angle) * r, base.gamma_multiply(a));
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

/// Carte « verre » : ombre, dégradé translucide, bordure et reflet supérieur.
pub fn glass(ctx: &egui::Context, rect: Rect, radius: f32) -> Vec<Shape> {
    let fill = linear(
        rect,
        160.0,
        vec![(0.0, theme::white_alpha(0.06)), (1.0, theme::white_alpha(0.02))],
    );
    vec![
        outer_shadow(rect, radius, Vec2::new(0.0, 20.0), 60.0, -20.0, theme::rgba(0, 0, 0, 0.6)),
        gradient_rect(ctx, rect, radius, &fill),
        Shape::Rect(RectShape::stroke(rect, cr(radius), Stroke::new(1.0, theme::BORDER), StrokeKind::Inside)),
        top_highlight(rect, radius, theme::white_alpha(0.05)),
    ]
}

/// Fond des fenêtres modales, plus opaque.
pub fn modal_glass(ctx: &egui::Context, rect: Rect, radius: f32) -> Vec<Shape> {
    let fill = linear(
        rect,
        160.0,
        vec![(0.0, theme::rgba(30, 33, 58, 0.95)), (1.0, theme::rgba(16, 18, 34, 0.96))],
    );
    vec![
        outer_shadow(rect, radius, Vec2::new(0.0, 20.0), 60.0, -20.0, theme::rgba(0, 0, 0, 0.6)),
        gradient_rect(ctx, rect, radius, &fill),
        Shape::Rect(RectShape::stroke(rect, cr(radius), Stroke::new(1.0, theme::BORDER), StrokeKind::Inside)),
        top_highlight(rect, radius, theme::white_alpha(0.05)),
    ]
}

/// Reflet clair sur le bord supérieur (`inset 0 1px 0 rgba(255,255,255,a)`).
pub fn top_highlight(rect: Rect, radius: f32, color: Color32) -> Shape {
    let y = rect.top() + 1.0;
    Shape::line_segment(
        [Pos2::new(rect.left() + radius, y), Pos2::new(rect.right() - radius, y)],
        Stroke::new(1.0, color),
    )
}

/// Animation « va-et-vient » CSS (`ease-in-out infinite`, 50 % au milieu du cycle).
pub fn ping_pong(time: f64, period: f64) -> f32 {
    let phase = (time / period).fract() as f32;
    let t = if phase < 0.5 { phase * 2.0 } else { 2.0 - phase * 2.0 };
    ease_in_out(t)
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `cubic-bezier(.2, .8, .2, 1)` approché : départ rapide, arrivée douce.
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// `cubic-bezier(.34, 1.56, .64, 1)` approché : léger dépassement (ressort).
pub fn spring(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158 * 1.2;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Fond de la fenêtre : couleur de base, trois halos colorés qui flottent
/// lentement (mélange « screen ») et grille estompée.
pub fn backdrop(painter: &Painter, rect: Rect, accent: Accent, time: f64) {
    painter.rect_filled(rect, 0.0, theme::BG);
    let o = rect.min.to_vec2();
    let (w, h) = (rect.width(), rect.height());

    let e1 = ping_pong(time, 22.0);
    let c1 = Pos2::new(190.0, 90.0) + Vec2::new(140.0, 90.0) * e1 + o;
    painter.add(soft_disc(c1, 310.0 * (1.0 + 0.12 * e1), 90.0, accent.a1.gamma_multiply(0.42), true));

    let e2 = ping_pong(time, 26.0);
    let c2 = Pos2::new(w - 140.0, h - 20.0) + Vec2::new(-160.0, -80.0) * e2 + o;
    painter.add(soft_disc(c2, 260.0 * (1.0 + 0.08 * e2), 90.0, accent.a2.gamma_multiply(0.32), true));

    let e3 = ping_pong(time, 30.0);
    let pink = Color32::from_rgb(0xec, 0x48, 0x99);
    let c3 = Pos2::new(0.45 * w, 0.4 * h) + Vec2::new(76.0, -76.0) * e3 + o;
    painter.add(soft_disc(
        c3,
        190.0 * (1.0 + 0.25 * e3),
        90.0,
        theme::lerp(accent.a1, pink, 0.5).gamma_multiply(0.2),
        true,
    ));

    grid(painter, rect);
}

/// Grille de 44 px, masquée par une ellipse centrée à (60 %, 40 %).
fn grid(painter: &Painter, rect: Rect) {
    const STEP: f32 = 44.0;
    let center = rect.min + Vec2::new(rect.width() * 0.6, rect.height() * 0.4);
    // `radial-gradient(ellipse at 60% 40%, #000 20%, transparent 75%)` : l'ellipse
    // va jusqu'au coin le plus éloigné.
    let radius = Vec2::new(rect.width() * 0.6, rect.height() * 0.6) * std::f32::consts::SQRT_2;
    let mask = |p: Pos2| {
        let d = ((p - center) / radius).length();
        1.0 - ((d - 0.2) / 0.55).clamp(0.0, 1.0)
    };
    let line = |a: Pos2, b: Pos2| {
        let alpha = mask(a.lerp(b, 0.5)) * 0.035;
        if alpha > 0.002 {
            painter.line_segment([a, b], Stroke::new(1.0, theme::white_alpha(alpha)));
        }
    };
    let mut x = rect.left();
    while x <= rect.right() {
        let mut y = rect.top();
        while y < rect.bottom() {
            line(Pos2::new(x, y), Pos2::new(x, y + STEP));
            y += STEP;
        }
        x += STEP;
    }
    let mut y = rect.top();
    while y <= rect.bottom() {
        let mut x = rect.left();
        while x < rect.right() {
            line(Pos2::new(x, y), Pos2::new(x + STEP, y));
            x += STEP;
        }
        y += STEP;
    }
}

/// Texte peint avec un dégradé (`background-clip: text`).
pub fn gradient_text(painter: &Painter, pos: Pos2, galley: std::sync::Arc<egui::Galley>, brush: Brush<'_>) {
    let shape = Shape::galley(pos, galley, Color32::WHITE);
    painter.add(gradient_shape(painter.ctx(), shape, brush));
}

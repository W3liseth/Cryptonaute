//! Widgets du thème : boutons, boutons-icônes, puces, étiquettes, interrupteurs,
//! champs de saisie et cartes « verre », reproduits d'après l'ancienne feuille
//! de style (tailles, rayons, couleurs, survol).

use eframe::egui::{
    self, text::LayoutJob, Color32, CursorIcon, Id, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2,
};
use eframe::epaint::RectShape;

use super::icons::{self, Icon};
use super::paint::{self, cr, WithOpacity};
use super::theme::{self, Accent, Weight};

/// Progression animée du survol d'un élément (transition CSS de 0,2 s).
pub fn hover_anim(ui: &Ui, id: Id, hovered: bool) -> f32 {
    ui.ctx().animate_bool_with_time(id.with("hover"), hovered, 0.2)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Default,
    Primary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
}

/// Bouton texte, avec icône facultative (`.btn`).
pub struct Button<'a> {
    label: &'a str,
    icon: Option<Icon>,
    kind: Kind,
    size: Size,
    enabled: bool,
    width: Option<f32>,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Button {
            label,
            icon: None,
            kind: Kind::Default,
            size: Size::Medium,
            enabled: true,
            width: None,
        }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }
    pub fn size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    /// Largeur imposée (le contenu est centré).
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    fn metrics(&self) -> (f32, f32, f32, f32) {
        // hauteur, marge horizontale, rayon, taille du texte
        match self.size {
            Size::Small => (32.0, 12.0, 10.0, 13.0),
            Size::Medium => (38.0, 16.0, 12.0, 14.0),
            Size::Large => (46.0, 22.0, 14.0, 15.0),
        }
    }

    pub fn show(self, ui: &mut Ui, accent: Accent) -> Response {
        let (height, pad, radius, font) = self.metrics();
        let text_color = match self.kind {
            Kind::Primary | Kind::Danger => Color32::WHITE,
            _ => theme::TEXT,
        };
        let galley = ui.painter().layout_job(theme::job(self.label, font, Weight::Semibold, text_color));
        let icon_w = if self.icon.is_some() { 16.0 + 8.0 } else { 0.0 };
        let content_w = galley.size().x + icon_w;
        let width = self.width.unwrap_or(content_w + pad * 2.0);
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let hover = hover_anim(ui, response.id, self.enabled && response.hovered());
        let pressed = self.enabled && response.is_pointer_button_down_on();
        // translateY(-1px) au survol, léger enfoncement au clic
        let rect = rect.translate(Vec2::new(0.0, if pressed { 0.0 } else { -hover }));
        let rect = if pressed { rect.shrink2(rect.size() * 0.015) } else { rect };
        let opacity = if self.enabled { 1.0 } else { 0.5 };
        let painter = ui.painter().clone().with_opacity(opacity);
        let ctx = ui.ctx();

        match self.kind {
            Kind::Primary | Kind::Danger => {
                let (c1, c2, glow) = if self.kind == Kind::Primary {
                    (accent.a1, accent.a2, theme::fade(accent.a1, 0.8 + 0.1 * hover))
                } else {
                    (theme::ROSE, Color32::from_rgb(0xe1, 0x1d, 0x48), theme::rgba(244, 63, 94, 0.7))
                };
                let lift = 1.0 + 0.1 * hover; // filter: brightness(1.1)
                let brighten = |c: Color32| {
                    let f = |v: u8| ((v as f32) * lift).min(255.0) as u8;
                    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
                };
                painter.add(paint::shadow(rect, radius, Vec2::new(0.0, 8.0 + 4.0 * hover), 24.0 + 6.0 * hover, -8.0, glow));
                let brush = paint::linear(rect, 135.0, vec![(0.0, brighten(c1)), (1.0, brighten(c2))]);
                painter.add(paint::gradient_rect(ctx, rect, radius, &brush));
                painter.add(paint::top_highlight(rect, radius, theme::white_alpha(0.25)));
            }
            Kind::Default | Kind::Ghost => {
                let base = if self.kind == Kind::Ghost { Color32::TRANSPARENT } else { theme::SURFACE };
                let fill = theme::lerp(base, theme::SURFACE_2, hover);
                let border = theme::lerp(theme::BORDER, theme::BORDER_STRONG, if self.kind == Kind::Ghost { 0.0 } else { hover });
                painter.add(RectShape::new(rect, cr(radius), fill, Stroke::new(1.0, border), StrokeKind::Inside));
            }
        }

        let start = rect.center().x - content_w / 2.0;
        if let Some(icon) = self.icon {
            let icon_rect = Rect::from_center_size(Pos2::new(start + 8.0, rect.center().y), Vec2::splat(16.0));
            icons::paint(ui, icon, icon_rect, theme::fade(text_color, opacity));
        }
        let pos = Pos2::new(start + icon_w, rect.center().y - galley.size().y / 2.0);
        painter.galley(pos, galley, text_color);
        if self.enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

/// Bouton carré à icône (`.icon-btn`).
pub fn icon_button(ui: &mut Ui, icon: Icon, small: bool, danger: bool, tooltip: &str) -> Response {
    let (size, radius, glyph) = if small { (32.0, 10.0, 16.0) } else { (40.0, 12.0, 18.0) };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hover = hover_anim(ui, response.id, response.hovered());
    let rect = rect.translate(Vec2::new(0.0, -hover));
    let (fill, border, color) = if danger {
        (
            theme::lerp(theme::SURFACE, theme::rgba(244, 63, 94, 0.15), hover),
            theme::lerp(theme::BORDER, theme::rgba(244, 63, 94, 0.35), hover),
            theme::lerp(theme::TEXT_2, theme::ROSE, hover),
        )
    } else {
        (
            theme::lerp(theme::SURFACE, theme::SURFACE_2, hover),
            theme::BORDER,
            theme::lerp(theme::TEXT_2, theme::TEXT, hover),
        )
    };
    ui.painter().add(RectShape::new(rect, cr(radius), fill, Stroke::new(1.0, border), StrokeKind::Inside));
    icons::paint_centered(ui, icon, rect.center(), glyph, color);
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Normal,
    Accent,
    Warn,
}

/// Puce arrondie avec icône (`.chip`).
pub fn chip(ui: &mut Ui, icon: Icon, text: &str, kind: ChipKind, accent: Accent) -> Response {
    let color = match kind {
        ChipKind::Normal => theme::TEXT_2,
        ChipKind::Accent => Color32::WHITE,
        ChipKind::Warn => theme::WARN_TEXT,
    };
    let galley = ui.painter().layout_job(theme::job(text, 12.0, Weight::Regular, color));
    let size = Vec2::new(10.0 + 13.0 + 6.0 + galley.size().x + 10.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    match kind {
        ChipKind::Accent => {
            let brush = paint::linear(
                rect,
                135.0,
                vec![(0.0, theme::fade(accent.a1, 0.55)), (1.0, theme::fade(accent.a2, 0.45))],
            );
            painter.add(paint::gradient_rect(ui.ctx(), rect, 13.0, &brush));
        }
        ChipKind::Warn => {
            painter.add(RectShape::new(
                rect,
                cr(13.0),
                theme::rgba(245, 158, 11, 0.12),
                Stroke::new(1.0, theme::rgba(245, 158, 11, 0.3)),
                StrokeKind::Inside,
            ));
        }
        ChipKind::Normal => {
            painter.add(RectShape::new(rect, cr(13.0), theme::SURFACE_2, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside));
        }
    }
    icons::paint_centered(ui, icon, Pos2::new(rect.left() + 10.0 + 6.5, rect.center().y), 13.0, color);
    painter.galley(Pos2::new(rect.left() + 29.0, rect.center().y - galley.size().y / 2.0), galley, color);
    response
}

/// Étiquette en police à chasse fixe (`.tag`).
pub fn tag(ui: &mut Ui, text: &str, fill: Color32, color: Color32) -> Response {
    let galley = ui.painter().layout_job(LayoutJob::single_section(text.to_string(), theme::mono(12.0, color)));
    let size = galley.size() + Vec2::new(16.0, 4.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, cr(6.0), fill);
    ui.painter().galley(rect.min + Vec2::new(8.0, 2.0), galley, color);
    response
}

/// Interrupteur (`.switch`) ; renvoie vrai si l'état a été inversé.
pub fn switch(ui: &mut Ui, id: Id, on: &mut bool, enabled: bool, accent: Accent) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(38.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(id.with("switch"), *on, 0.35);
    let opacity = if enabled { 1.0 } else { 0.5 };
    let painter = ui.painter().clone().with_opacity(opacity);
    if t > 0.0 {
        painter.add(paint::shadow(rect, 11.0, Vec2::ZERO, 14.0, -2.0, theme::fade(accent.a1, 0.7 * t)));
    }
    painter.add(RectShape::new(
        rect,
        cr(11.0),
        Color32::from_rgb(0x2a, 0x2d, 0x47),
        Stroke::new(1.0, theme::fade(theme::BORDER_STRONG, 1.0 - t)),
        StrokeKind::Inside,
    ));
    if t > 0.0 {
        let brush = paint::linear(rect, 135.0, vec![(0.0, accent.a1), (1.0, accent.a2)]);
        painter.add(paint::gradient_rect(ui.ctx(), rect, 11.0, &brush).multiply_opacity_shape(t));
    }
    let knob_x = rect.left() + 2.0 + 8.0 + 16.0 * paint::spring(t);
    let knob = Pos2::new(knob_x, rect.center().y);
    painter.add(paint::shadow(
        Rect::from_center_size(knob + Vec2::new(0.0, 2.0), Vec2::splat(16.0)),
        8.0,
        Vec2::ZERO,
        6.0,
        0.0,
        theme::rgba(0, 0, 0, 0.4),
    ));
    painter.circle_filled(knob, 8.0, theme::lerp(Color32::from_rgb(0xc7, 0xcb, 0xe6), Color32::WHITE, t));
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Opacité appliquée à une forme déjà construite.
trait ShapeOpacity {
    fn multiply_opacity_shape(self, opacity: f32) -> Shape;
}

impl ShapeOpacity for Shape {
    fn multiply_opacity_shape(mut self, opacity: f32) -> Shape {
        if let Shape::Mesh(mesh) = &mut self {
            let mesh = std::sync::Arc::make_mut(mesh);
            for v in &mut mesh.vertices {
                v.color = v.color.gamma_multiply(opacity);
            }
        }
        self
    }
}

/// Cadre de champ de saisie : fond sombre, bordure et halo de focus.
pub fn field_frame(ui: &Ui, rect: Rect, radius: f32, focused: f32, invalid: bool, accent: Accent, fill: Color32) {
    let painter = ui.painter();
    if focused > 0.0 {
        painter.add(RectShape::new(
            rect.expand(4.0 * focused),
            cr(radius + 4.0),
            theme::fade(accent.a1, 0.15 * focused),
            Stroke::NONE,
            StrokeKind::Inside,
        ));
    }
    let border = if invalid {
        theme::rgba(244, 63, 94, 0.6)
    } else {
        theme::lerp(theme::BORDER, theme::fade(accent.a1, 0.6), focused)
    };
    painter.add(RectShape::new(rect, cr(radius), fill, Stroke::new(1.0, border), StrokeKind::Inside));
}

/// Champ de saisie sur une ligne, dans un cadre du thème.
pub struct TextField<'a> {
    pub id: Id,
    pub text: &'a mut String,
    pub hint: &'a str,
    pub height: f32,
    pub radius: f32,
    pub font_size: f32,
    pub fill: Color32,
    pub invalid: bool,
    pub icon: Option<Icon>,
    pub char_limit: usize,
}

impl TextField<'_> {
    pub fn show(self, ui: &mut Ui, accent: Accent) -> egui::Response {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, self.height), Sense::hover());
        let focused = ui.memory(|m| m.has_focus(self.id));
        let f = ui.ctx().animate_bool_with_time(self.id.with("focus"), focused, 0.2);
        field_frame(ui, rect, self.radius, f, self.invalid, accent, self.fill);
        let mut inner = rect.shrink2(Vec2::new(if self.icon.is_some() { 12.0 } else { 14.0 }, 0.0));
        if let Some(icon) = self.icon {
            icons::paint_centered(ui, icon, Pos2::new(inner.left() + 8.0, rect.center().y), 16.0, theme::MUTED);
            inner.min.x += 24.0;
        }
        let font = egui::FontId::proportional(self.font_size);
        let row = ui.fonts_mut(|f| f.row_height(&font));
        let edit_rect = Rect::from_min_size(Pos2::new(inner.left(), rect.center().y - row / 2.0 - 2.0), Vec2::new(inner.width(), row + 4.0));
        let hint = egui::RichText::new(self.hint).color(theme::MUTED).size(self.font_size);
        let mut child = ui.new_child(UiBuilder::new().max_rect(edit_rect));
        child.add(
            egui::TextEdit::singleline(self.text)
                .id(self.id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .font(font)
                .text_color(theme::TEXT)
                .hint_text(hint)
                .char_limit(self.char_limit)
                .desired_width(edit_rect.width()),
        )
    }
}

/// Petit bouton « Copier » (`.copy`).
pub fn copy_button(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
    let hover = hover_anim(ui, response.id, response.hovered());
    if hover > 0.0 {
        ui.painter().rect_filled(rect, cr(6.0), theme::fade(theme::SURFACE_2, hover));
    }
    icons::paint_centered(ui, Icon::Copy, rect.center(), 14.0, theme::lerp(theme::MUTED, theme::TEXT, hover));
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Copier")
}

/// Contenu dans une carte « verre » de hauteur ajustée au contenu.
pub fn glass_card<R>(ui: &mut Ui, padding: Vec2, radius: f32, add: impl FnOnce(&mut Ui) -> R) -> (Rect, R) {
    let bg = ui.painter().add(Shape::Noop);
    let outer = ui.available_rect_before_wrap();
    let inner_rect = Rect::from_min_max(outer.min + padding, Pos2::new(outer.right() - padding.x, outer.bottom()));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner_rect).layout(egui::Layout::top_down(egui::Align::Min)));
    let result = add(&mut child);
    let used = child.min_rect();
    let rect = Rect::from_min_max(outer.min, Pos2::new(outer.right(), used.bottom() + padding.y));
    ui.allocate_rect(rect, Sense::hover());
    ui.painter().set(bg, Shape::Vec(paint::glass(ui.ctx(), rect, radius)));
    (rect, result)
}

/// Texte mis en forme, qui n'occupe que sa taille et respecte la largeur de
/// retour à la ligne de `job` (contrairement à `Label` dans une ligne).
pub fn text(ui: &mut Ui, job: LayoutJob) -> Response {
    let galley = ui.painter().layout_job(job);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    // Un texte centré ou aligné à droite a une origine décalée dans son galley.
    ui.painter().galley(rect.min - galley.rect.min.to_vec2(), galley, theme::TEXT);
    response
}


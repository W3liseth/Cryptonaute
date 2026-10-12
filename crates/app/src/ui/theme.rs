//! Thème « verre dépoli » : palette, polices et mise en forme du texte.
//!
//! Reprend les variables de l'ancienne feuille de style (`--bg`, `--surface`,
//! `--a1`/`--a2`…). Les couleurs d'accent suivent l'état du tunnel affiché et
//! changent en fondu, comme la transition CSS de 1,2 s.

use eframe::egui::{
    self, text::LayoutJob, Color32, FontData, FontDefinitions, FontFamily, FontId, TextFormat,
};
use eframe::epaint::text::VariationCoords;

// ---------- Palette ----------
pub const BG: Color32 = Color32::from_rgb(0x09, 0x0a, 0x13);
pub const TEXT: Color32 = Color32::from_rgb(0xec, 0xee, 0xfe);
pub const TEXT_2: Color32 = Color32::from_rgb(0xb8, 0xbc, 0xd9);
pub const MUTED: Color32 = Color32::from_rgb(0x7f, 0x84, 0xa6);
pub const VIOLET: Color32 = Color32::from_rgb(0x7c, 0x5c, 0xff);
pub const VIOLET_LIGHT: Color32 = Color32::from_rgb(0xa7, 0x8b, 0xfa);
pub const CYAN: Color32 = Color32::from_rgb(0x22, 0xd3, 0xee);
pub const GREEN: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
pub const TEAL: Color32 = Color32::from_rgb(0x2d, 0xd4, 0xbf);
pub const AMBER: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const ROSE: Color32 = Color32::from_rgb(0xf4, 0x3f, 0x5e);
pub const WARN_TEXT: Color32 = Color32::from_rgb(0xfc, 0xd3, 0x4d);
pub const ERROR_TEXT: Color32 = Color32::from_rgb(0xfd, 0xa4, 0xaf);
pub const MINT: Color32 = Color32::from_rgb(0x6e, 0xe7, 0xb7);

/// `rgba(255,255,255,a)` en alpha prémultiplié.
pub const fn white_alpha(a: f32) -> Color32 {
    let v = (a * 255.0 + 0.5) as u8;
    Color32::from_rgba_premultiplied(v, v, v, v)
}
pub const SURFACE: Color32 = white_alpha(0.045);
pub const SURFACE_2: Color32 = white_alpha(0.07);
pub const SURFACE_3: Color32 = white_alpha(0.10);
pub const BORDER: Color32 = white_alpha(0.08);
pub const BORDER_STRONG: Color32 = white_alpha(0.14);

/// Couleur `rgba(r,g,b,a)` (composantes non prémultipliées).
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
}

/// Même couleur avec une opacité relative (`color-mix(... x%, transparent)`).
pub fn fade(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Interpolation linéaire entre deux couleurs opaques.
pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

/// `hsl(h s% l% / a)`.
pub fn hsl(h: f32, s: f32, l: f32, a: f32) -> Color32 {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(to(r), to(g), to(b), (a * 255.0).round() as u8)
}

// ---------- État et couleurs d'accent ----------

/// État affiché (tunnel sélectionné), qui pilote les couleurs d'accent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Disconnected,
    Connecting,
    Disconnecting,
    Connected,
    Error,
}

impl Mood {
    pub fn accents(self) -> (Color32, Color32) {
        match self {
            Mood::Connected => (GREEN, TEAL),
            Mood::Connecting | Mood::Disconnecting => {
                (Color32::from_rgb(0x8b, 0x5c, 0xf6), Color32::from_rgb(0x38, 0xbd, 0xf8))
            }
            Mood::Error => (ROSE, Color32::from_rgb(0xfb, 0x92, 0x3c)),
            Mood::Disconnected => (VIOLET, CYAN),
        }
    }
}

/// Couleurs d'accent animées (`--a1`, `--a2`).
#[derive(Clone, Copy)]
pub struct Accent {
    pub a1: Color32,
    pub a2: Color32,
}

impl Accent {
    /// Fondu de 1,2 s vers les couleurs de `mood`.
    pub fn animate(ctx: &egui::Context, mood: Mood) -> Self {
        let (t1, t2) = mood.accents();
        let channel = |name: &str, c: Color32| {
            let anim = |i: usize, v: u8| {
                ctx.animate_value_with_time(egui::Id::new(("accent", name, i)), v as f32, 1.2)
                    .round() as u8
            };
            Color32::from_rgb(anim(0, c.r()), anim(1, c.g()), anim(2, c.b()))
        };
        Accent {
            a1: channel("a1", t1),
            a2: channel("a2", t2),
        }
    }
}

// ---------- Polices ----------

/// Graisses utilisées par l'interface.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Semibold,
    Bold,
}

impl Weight {
    fn value(self) -> f32 {
        match self {
            Weight::Regular => 400.0,
            Weight::Semibold => 600.0,
            Weight::Bold => 700.0,
        }
    }
    fn family(self) -> FontFamily {
        match self {
            Weight::Regular => FontFamily::Proportional,
            Weight::Semibold => FontFamily::Name("semibold".into()),
            Weight::Bold => FontFamily::Name("bold".into()),
        }
    }
}

/// Familles recherchées, dans l'ordre de l'ancienne pile CSS.
const UI_FAMILIES: &[&str] = &[
    "Segoe UI Variable Display",
    "Segoe UI Variable",
    "Segoe UI",
    "SF Pro Display",
    "SF Pro Text",
    "SF Pro",
    "Helvetica Neue",
    "Inter",
    "Cantarell",
    "Ubuntu",
    "Noto Sans",
    "DejaVu Sans",
];
const MONO_FAMILIES: &[&str] = &[
    "Cascadia Mono",
    "Cascadia Code",
    "SF Mono",
    "Menlo",
    "JetBrains Mono",
    "DejaVu Sans Mono",
    "Consolas",
    "Liberation Mono",
];

/// Charge la meilleure police disponible parmi `families` pour une graisse.
fn system_font(db: &fontdb::Database, families: &[&str], weight: u16) -> Option<FontData> {
    let names: Vec<fontdb::Family> = families.iter().map(|f| fontdb::Family::Name(f)).collect();
    let id = db.query(&fontdb::Query {
        families: &names,
        weight: fontdb::Weight(weight),
        ..Default::default()
    })?;
    db.with_face_data(id, |data, index| {
        let mut font = FontData::from_owned(data.to_vec());
        font.index = index;
        font
    })
}

/// Installe les polices du système (repli sur les polices intégrées à egui).
pub fn install_fonts(ctx: &egui::Context) {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut fonts = FontDefinitions::default();
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_defaults = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    // Chaque famille commence par la police du système trouvée, puis les polices
    // intégrées à egui (repli, symboles).
    let mut family = |key: &str, data: Option<FontData>, fallback: &[String]| {
        let mut list = Vec::new();
        if let Some(data) = data {
            fonts.font_data.insert(key.to_string(), data.into());
            list.push(key.to_string());
        }
        list.extend(fallback.iter().cloned());
        list
    };
    let regular = family("ui-regular", system_font(&db, UI_FAMILIES, 400), &defaults);
    let semibold = family("ui-semibold", system_font(&db, UI_FAMILIES, 600), &regular);
    let bold = family("ui-bold", system_font(&db, UI_FAMILIES, 700), &semibold);
    let mono = family("ui-mono", system_font(&db, MONO_FAMILIES, 400), &mono_defaults);
    fonts.families.insert(FontFamily::Proportional, regular);
    fonts.families.insert(Weight::Semibold.family(), semibold);
    fonts.families.insert(Weight::Bold.family(), bold);
    fonts.families.insert(FontFamily::Monospace, mono);
    ctx.set_fonts(fonts);
}

/// Format de texte : taille, graisse (axe `wght` des polices variables) et couleur.
pub fn format(size: f32, weight: Weight, color: Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::new(size, weight.family()),
        color,
        coords: VariationCoords::new([(b"wght", weight.value())]),
        ..Default::default()
    }
}

pub fn mono(size: f32, color: Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::monospace(size),
        color,
        ..Default::default()
    }
}

/// Texte d'une seule mise en forme.
pub fn job(text: impl Into<String>, size: f32, weight: Weight, color: Color32) -> LayoutJob {
    LayoutJob::single_section(text.into(), format(size, weight, color))
}

/// Petit titre en capitales espacées (`text-transform: uppercase; letter-spacing`).
pub fn caps(text: &str, size: f32, spacing: f32, color: Color32) -> LayoutJob {
    let mut fmt = format(size, Weight::Semibold, color);
    fmt.extra_letter_spacing = spacing;
    LayoutJob::single_section(text.to_uppercase(), fmt)
}

/// Mise en forme du thème egui (sélection, barres de défilement, champs).
pub fn install_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.global_style_mut(|style| {
        style.visuals.panel_fill = Color32::TRANSPARENT;
        style.visuals.window_fill = Color32::TRANSPARENT;
        style.visuals.extreme_bg_color = Color32::TRANSPARENT;
        style.visuals.override_text_color = None;
        style.visuals.selection.bg_fill = rgba(124, 92, 255, 0.40);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, TEXT);
        style.visuals.text_cursor.stroke = egui::Stroke::new(1.5, Color32::WHITE);
        style.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
        style.visuals.widgets.inactive.fg_stroke.color = TEXT;
        style.visuals.hyperlink_color = CYAN;
        style.spacing.item_spacing = egui::vec2(0.0, 0.0);
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 8.0,
            floating: true,
            floating_width: 8.0,
            floating_allocated_width: 0.0,
            foreground_color: false,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: 0.6,
            active_handle_opacity: 0.9,
            interact_handle_opacity: 1.0,
            ..egui::style::ScrollStyle::solid()
        };
        style.visuals.widgets.inactive.bg_fill = SURFACE_3;
        style.visuals.widgets.hovered.bg_fill = white_alpha(0.16);
        style.visuals.widgets.active.bg_fill = white_alpha(0.2);
        style.interaction.selectable_labels = false;
        style.url_in_tooltip = false;
    });
}

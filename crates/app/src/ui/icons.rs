//! Icônes vectorielles, reprises telles quelles de l'ancienne interface (traits
//! arrondis de 2 px sur une grille de 24). Elles sont dessinées en blanc puis
//! teintées, comme `stroke: currentColor` en CSS.

use std::borrow::Cow;

use eframe::egui::{self, Color32, Rect};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Search,
    Import,
    Plus,
    Update,
    Gear,
    Warn,
    Edit,
    Trash,
    Power,
    ArrowDown,
    ArrowUp,
    Globe,
    Split,
    Server,
    Check,
    Close,
    Info,
    Copy,
    File,
    Key,
    Refresh,
    Notes,
    Tray,
    Boot,
    WinMinimize,
    WinMaximize,
    WinClose,
}

/// Tracés SVG (éléments enfants de `<svg>`), et taille de la grille.
fn paths(icon: Icon) -> (&'static str, u32, f32) {
    use Icon::*;
    match icon {
        Search => (r#"<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>"#, 24, 2.0),
        Import => (r#"<path d="M12 3v12M7 10l5 5 5-5M4 19h16"/>"#, 24, 2.0),
        Plus => (r#"<path d="M12 5v14M5 12h14"/>"#, 24, 2.0),
        Update => (r#"<path d="M12 19V7M6.5 12.5L12 7l5.5 5.5"/><path d="M5 3h14"/>"#, 24, 2.0),
        Gear => (
            r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 00.3 1.8l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.8-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.8.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.8 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.8l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.8.3H9a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.8-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.8V9a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z"/>"#,
            24,
            2.0,
        ),
        Warn => (
            r#"<path d="M12 9v4M12 17h.01M10.3 3.9L1.8 18a2 2 0 001.7 3h17a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z"/>"#,
            24,
            2.0,
        ),
        Edit => (r#"<path d="M4 20h4L19 9l-4-4L4 16v4zM13.5 6.5l4 4"/>"#, 24, 2.0),
        Trash => (r#"<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>"#, 24, 2.0),
        Power => (r#"<path d="M12 3v8"/><path d="M6.3 6.8a8 8 0 1011.4 0"/>"#, 24, 2.4),
        ArrowDown => (r#"<path d="M12 4v16M6 14l6 6 6-6"/>"#, 24, 2.0),
        ArrowUp => (r#"<path d="M12 20V4M6 10l6-6 6 6"/>"#, 24, 2.0),
        Globe => (
            r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.5 2.7 3.8 5.7 3.8 9s-1.3 6.3-3.8 9c-2.5-2.7-3.8-5.7-3.8-9S9.5 5.7 12 3z"/>"#,
            24,
            2.0,
        ),
        Split => (r#"<path d="M6 3v6a3 3 0 003 3h6a3 3 0 013 3v6M6 21v-6"/>"#, 24, 2.0),
        Server => (
            r#"<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/>"#,
            24,
            2.0,
        ),
        Check => (r#"<path d="M5 12l5 5L20 7"/>"#, 24, 2.6),
        Close => (r#"<path d="M6 6l12 12M18 6L6 18"/>"#, 24, 2.0),
        Info => (r#"<path d="M12 8h.01M11 12h1v5h1"/>"#, 24, 2.6),
        Copy => (r#"<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 012-2h8"/>"#, 24, 2.0),
        File => (r#"<path d="M14 3H6a2 2 0 00-2 2v14a2 2 0 002 2h12a2 2 0 002-2V9z"/><path d="M14 3v6h6"/>"#, 24, 2.0),
        Key => (r#"<circle cx="8" cy="15" r="4"/><path d="M10.8 12.2L20 3M17 6l3 3"/>"#, 24, 2.0),
        Refresh => (r#"<path d="M21 12a9 9 0 11-3-6.7L21 8"/><path d="M21 3v5h-5"/>"#, 24, 2.0),
        Notes => (
            r#"<path d="M14 3H7a2 2 0 00-2 2v14a2 2 0 002 2h10a2 2 0 002-2V8z"/><path d="M14 3v5h5M9 13h6M9 17h4"/>"#,
            24,
            2.0,
        ),
        Tray => (
            r#"<rect x="3" y="14" width="18" height="7" rx="2"/><path d="M7 17.5h.01M12 3v8M8.5 7.5L12 11l3.5-3.5"/>"#,
            24,
            2.0,
        ),
        Boot => (r#"<path d="M12 2v6M18.4 5.6l-1.4 1.4M22 12h-6M5.6 5.6L7 7M2 12h6"/><circle cx="12" cy="15" r="5"/>"#, 24, 2.0),
        WinMinimize => (r#"<path d="M2 6h8"/>"#, 12, 1.2),
        WinMaximize => (r#"<rect x="2.5" y="2.5" width="7" height="7" rx="1.2"/>"#, 12, 1.2),
        WinClose => (r#"<path d="M3 3l6 6M9 3l-6 6"/>"#, 12, 1.2),
    }
}

/// Document SVG complet de l'icône, tracé en blanc (teinté ensuite).
fn svg(icon: Icon) -> String {
    let (body, grid, width) = paths(icon);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {grid} {grid}" width="{grid}" height="{grid}" fill="none" stroke="#fff" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##
    )
}

pub fn source_of(icon: Icon) -> egui::ImageSource<'static> {
    // Les documents sont construits une fois et gardés pour la durée du programme.
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, &'static [u8]>>> =
        std::sync::OnceLock::new();
    let key = format!("{icon:?}");
    let bytes = {
        let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        *cache
            .entry(key.clone())
            .or_insert_with(|| Box::leak(svg(icon).into_bytes().into_boxed_slice()))
    };
    egui::ImageSource::Bytes {
        uri: Cow::Owned(format!("bytes://icon-{key}.svg")),
        bytes: egui::load::Bytes::Static(bytes),
    }
}

/// Dessine l'icône dans `rect`, de la couleur `color`.
pub fn paint(ui: &egui::Ui, icon: Icon, rect: Rect, color: Color32) {
    egui::Image::new(source_of(icon))
        .fit_to_exact_size(rect.size())
        .tint(color)
        .paint_at(ui, rect);
}

/// Dessine l'icône (taille `size`) centrée en `center`.
pub fn paint_centered(ui: &egui::Ui, icon: Icon, center: egui::Pos2, size: f32, color: Color32) {
    paint(ui, icon, Rect::from_center_size(center, egui::vec2(size, size)), color);
}

/// Logo : bouclier blanc et éclair sur un carré arrondi en dégradé violet → cyan.
pub const LOGO_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32"><defs><linearGradient id="lg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#7c5cff"/><stop offset="1" stop-color="#22d3ee"/></linearGradient></defs><rect x="1" y="1" width="30" height="30" rx="8" fill="url(#lg)"/><path d="M16 6.5c2.6 1.4 5 2 7.3 2.2v6.4c0 5-3 8.6-7.3 10.5-4.3-1.9-7.3-5.5-7.3-10.5V8.7c2.3-.2 4.7-.8 7.3-2.2z" fill="#fff"/><path d="M17 10l-4.6 7.2h3.4l-.9 5.2 5-7.7h-3.4z" fill="url(#lg)"/></svg>"##;

/// Bouclier de l'écran d'accueil (rempli, teinté à la couleur d'accent).
pub const SHIELD_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><path d="M32 8c5.4 2.9 10.4 4.2 15.2 4.6v13.3c0 10.4-6.2 17.9-15.2 21.9-9-4-15.2-11.5-15.2-21.9V12.6C21.6 12.2 26.6 10.9 32 8z" fill="#fff"/></svg>"##;

pub fn paint_svg(ui: &egui::Ui, uri: &'static str, bytes: &'static [u8], rect: Rect, tint: Color32) {
    egui::Image::new(egui::ImageSource::Bytes {
        uri: Cow::Borrowed(uri),
        bytes: egui::load::Bytes::Static(bytes),
    })
    .fit_to_exact_size(rect.size())
    .tint(tint)
    .paint_at(ui, rect);
}

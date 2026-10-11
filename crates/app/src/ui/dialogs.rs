//! Fenêtres modales : éditeur, confirmation de suppression, options, patch
//! notes et mise à jour. Fond assombri, apparition animée (`popIn`), en-tête,
//! corps défilant et pied de fenêtre, comme l'ancienne interface.

use eframe::egui::{
    self, text::LayoutJob, Align, Color32, Id, Layout, Order, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    Ui, UiBuilder, Vec2,
};
use eframe::epaint::RectShape;

use super::app::{App, EditorState, Modal, ModalState, Validation};
use super::changelog::{self, Span};
use super::highlight;
use super::icons::{self, Icon};
use super::paint::{self, cr};
use super::theme::{self, Accent, Weight};
use super::widgets::{self, Button, Kind, Size, TextField};

/// Action demandée depuis une modale (appliquée après le dessin).
enum Action {
    None,
    Close,
    SaveEditor,
    LoadFile,
    NewKeys,
    Delete(String),
    ShowTray(bool),
    Autostart(bool),
    AutoUpdate(bool),
    CheckUpdate,
    PatchNotes,
    OpenPage,
    InstallUpdate,
}

pub fn show_modals(app: &mut App, ctx: &egui::Context, accent: Accent) {
    // Retire les modales dont l'animation de fermeture est terminée.
    let now = app.now;
    app.modals.retain(|m| m.closing.is_none_or(|t| now - t < 0.2));
    let count = app.modals.len();
    let mut actions = Vec::new();
    let mut modals = std::mem::take(&mut app.modals);
    let top = modals.iter().rposition(|m| m.closing.is_none());
    for (i, m) in modals.iter_mut().enumerate() {
        let action = show_one(app, ctx, i, m, Some(i) == top, accent);
        actions.push((i, action));
    }
    // Les modales ouvertes pendant le dessin (ex. Patch notes) passent au-dessus.
    modals.append(&mut app.modals);
    app.modals = modals;
    let _ = count;
    for (i, action) in actions {
        apply(app, i, action);
    }
}

fn apply(app: &mut App, index: usize, action: Action) {
    match action {
        Action::None => {}
        Action::Close => {
            if let Some(m) = app.modals.get_mut(index) {
                if !(matches!(m.modal, Modal::Update) && app.update_ui.installing) && m.closing.is_none() {
                    m.closing = Some(app.now);
                }
            }
        }
        Action::SaveEditor => app.save_editor(),
        Action::LoadFile => app.pick_editor_file(),
        Action::NewKeys => {
            if let Some(ed) = app.editor_mut() {
                App::regenerate_keys(ed);
            }
            app.toast_for(
                "Nouvelle paire de clés générée — communiquez la clé publique à votre serveur",
                super::app::ToastKind::Info,
                5.0,
            );
        }
        Action::Delete(name) => app.confirm_delete(name),
        Action::ShowTray(v) => app.set_show_tray(v),
        Action::Autostart(v) => app.set_autostart(v),
        Action::AutoUpdate(v) => app.set_auto_update(v),
        Action::CheckUpdate => app.check_update_now(),
        Action::PatchNotes => app.open_patch_notes(),
        Action::OpenPage => {
            let page = app
                .update
                .as_ref()
                .map(|u| u.page.clone())
                .unwrap_or_else(|| "https://github.com/W3liseth/Cryptonaute/releases".into());
            if let Err(e) = crate::updater::open_page(&page) {
                app.toast(e, super::app::ToastKind::Error);
            }
        }
        Action::InstallUpdate => app.install_update(),
    }
}

/// Progression d'ouverture (0 → 1) puis de fermeture (1 → 0).
fn presence(app: &App, m: &ModalState) -> (f32, f32) {
    match m.closing {
        Some(t) => {
            let p = ((app.now - t) / 0.2).clamp(0.0, 1.0) as f32;
            (1.0 - paint::ease_out(p), 8.0 * p)
        }
        None => {
            let p = ((app.now - m.opened) / 0.4).clamp(0.0, 1.0) as f32;
            (paint::ease_out(((app.now - m.opened) / 0.25).clamp(0.0, 1.0) as f32), 16.0 * (1.0 - paint::spring(p)))
        }
    }
}

fn show_one(app: &mut App, ctx: &egui::Context, index: usize, m: &mut ModalState, is_top: bool, accent: Accent) -> Action {
    let (alpha, dy) = presence(app, m);
    let screen = ctx.content_rect();
    let preferred: f32 = match &m.modal {
        Modal::Editor(_) => 760.0,
        Modal::Confirm { .. } => 420.0,
        _ => 560.0,
    };
    let width = preferred.min(screen.width() - 48.0);
    let max_h = screen.height() - 48.0;
    let mut action = Action::None;
    let open = m.closing.is_none();

    // Calque plein écran : voile sombre, puis carte centrée. Il intercepte les
    // clics destinés à l'interface en dessous.
    egui::Area::new(Id::new(("modal", index)))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(open)
        .show(ctx, |ui| {
            let (full, backdrop) = ui.allocate_exact_size(screen.size(), if open { Sense::click() } else { Sense::hover() });
            ui.painter().rect_filled(full, 0.0, theme::rgba(5, 6, 14, 0.6 * alpha));

            // Hauteur mesurée à l'image précédente, pour centrer la carte.
            let height_id = Id::new(("modal-height", index));
            let last_h = ui.ctx().memory(|mem| mem.data.get_temp::<f32>(height_id)).unwrap_or(420.0).min(max_h);
            let min = Pos2::new(screen.center().x - width / 2.0, screen.center().y - last_h / 2.0 + dy);
            let mut card = ui.new_child(
                UiBuilder::new()
                    .max_rect(Rect::from_min_size(min, Vec2::new(width, max_h)))
                    .layout(Layout::top_down(Align::Min)),
            );
            card.set_opacity(alpha);
            card.set_width(width);
            let bg = card.painter().add(Shape::Noop);
            match &mut m.modal {
                Modal::Editor(ed) => action = editor(app, &mut card, ed, accent, max_h),
                Modal::Confirm { name } => action = confirm(app, &mut card, name, accent),
                Modal::Options => action = options(app, &mut card, accent),
                Modal::Notes(releases) => action = notes(&mut card, releases, accent, app.now - m.opened, max_h),
                Modal::Update => action = update(app, &mut card, accent, max_h),
            }
            let rect = Rect::from_min_size(min, Vec2::new(width, card.min_rect().height()));
            card.painter().set(bg, Shape::Vec(paint::modal_glass(card.ctx(), rect, 18.0)));
            ui.ctx().memory_mut(|mem| mem.data.insert_temp(height_id, rect.height()));

            // Clic sur le fond (en dehors de la carte) : ferme la modale du dessus.
            let outside = ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)));
            if is_top && open && outside && backdrop.hovered() && matches!(action, Action::None) {
                action = Action::Close;
            }
        });
    action
}

// ---------- Éléments communs ----------

fn header(ui: &mut Ui, title: &str) -> bool {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 68.0), Sense::hover());
    let g = ui.painter().layout_job(theme::job(title, 18.0, Weight::Bold, theme::TEXT));
    ui.painter().galley(Pos2::new(rect.left() + 22.0, rect.center().y - g.size().y / 2.0), g, theme::TEXT);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, theme::BORDER));
    let close = Rect::from_center_size(Pos2::new(rect.right() - 22.0 - 16.0, rect.center().y), Vec2::splat(32.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(close));
    widgets::icon_button(&mut child, Icon::Close, true, false, "").clicked()
}

/// Corps de la modale, avec défilement au-delà de `max_h`.
fn body<R>(ui: &mut Ui, max_h: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::ScrollArea::vertical()
        .id_salt("modal-body")
        .max_height(max_h)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add_space(18.0);
            let r = ui
                .horizontal(|ui| {
                    ui.add_space(22.0);
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width() - 22.0);
                        add(ui)
                    })
                    .inner
                })
                .inner;
            ui.add_space(18.0);
            r
        })
        .inner
}

/// Pied de fenêtre : bordure supérieure puis boutons alignés à droite.
fn footer(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    let top = ui.cursor().min.y;
    ui.painter().hline(ui.max_rect().x_range(), top + 0.5, Stroke::new(1.0, theme::BORDER));
    let rect = Rect::from_min_size(Pos2::new(ui.max_rect().left() + 22.0, top + 16.0), Vec2::new(width - 44.0, 38.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::right_to_left(Align::Center)));
    child.spacing_mut().item_spacing.x = 10.0;
    add(&mut child);
    ui.allocate_exact_size(Vec2::new(width, 16.0 + 38.0 + 16.0), Sense::hover());
}

fn note(ui: &mut Ui, text: &str) {
    let mut job = theme::job(text, 12.0, Weight::Regular, theme::MUTED);
    job.sections[0].format.line_height = Some(18.0);
    job.wrap.max_width = ui.available_width() - 8.0;
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        widgets::text(ui, job);
    });
}

// ---------- Éditeur ----------

fn editor(app: &App, ui: &mut Ui, ed: &mut EditorState, accent: Accent, max_h: f32) -> Action {
    let mut action = Action::None;
    if header(ui, &ed.title) {
        action = Action::Close;
    }
    body(ui, max_h - 68.0 - 70.0, |ui| {
        // Nom
        widgets::text(ui, theme::caps("Nom du tunnel", 12.0, 1.2, theme::MUTED));
        ui.add_space(6.0);
        let name_id = Id::new("editor-name");
        if ed.focus_name {
            ui.memory_mut(|m| m.request_focus(name_id));
            ed.focus_name = false;
        }
        TextField {
            id: name_id,
            text: &mut ed.name,
            hint: "ex. bureau-paris",
            height: 42.0,
            radius: 12.0,
            font_size: 15.0,
            fill: theme::rgba(0, 0, 0, 0.25),
            invalid: ed.name_error.is_some(),
            icon: None,
            char_limit: 32,
        }
        .show(ui, accent);
        ui.add_space(6.0);
        let err = ed.name_error.clone().unwrap_or_default();
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
        let g = ui.painter().layout_job(theme::job(err, 12.0, Weight::Regular, theme::ERROR_TEXT));
        ui.painter().galley(r.min, g, theme::ERROR_TEXT);
        ui.add_space(14.0);

        // Outils
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if Button::new("Charger un fichier").icon(Icon::File).kind(Kind::Ghost).size(Size::Small).show(ui, accent).clicked() {
                action = Action::LoadFile;
            }
            if Button::new("Nouvelle paire de clés").icon(Icon::Key).kind(Kind::Ghost).size(Size::Small).show(ui, accent).clicked() {
                action = Action::NewKeys;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::text(ui, theme::job("Format wg-quick (.conf)", 12.0, Weight::Regular, theme::MUTED));
            });
        });
        ui.add_space(14.0);

        // Éditeur de code avec coloration
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 290.0), Sense::hover());
        let code_id = Id::new("editor-code");
        let focused = ui.memory(|m| m.has_focus(code_id));
        let f = ui.ctx().animate_bool_with_time(code_id.with("focus"), focused, 0.2);
        widgets::field_frame(ui, rect, 14.0, f * 0.9, false, accent, theme::rgba(0, 0, 0, 0.35));
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink(1.0)));
        egui::ScrollArea::both().id_salt("code-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
                ui.fonts_mut(|f| f.layout_job(highlight::highlight(text.as_str())))
            };
            ui.add(
                egui::TextEdit::multiline(&mut ed.config)
                    .id(code_id)
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 14)))
                    .font(egui::FontId::monospace(highlight::FONT_SIZE))
                    .code_editor()
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .desired_rows(11)
                    .min_size(rect.size() - Vec2::splat(2.0))
                    .layouter(&mut layouter),
            );
        });
        ui.add_space(14.0);

        app.validate_editor(ed);
        validation_box(ui, &ed.validation);
    });
    footer(ui, |ui| {
        let label = if ed.previous.is_some() { "Enregistrer" } else { "Ajouter le tunnel" };
        if Button::new(label).kind(Kind::Primary).enabled(App::editor_can_save(ed)).show(ui, accent).clicked() {
            action = Action::SaveEditor;
        }
        if Button::new("Annuler").kind(Kind::Ghost).show(ui, accent).clicked() {
            action = Action::Close;
        }
    });
    action
}

fn validation_box(ui: &mut Ui, validation: &Validation) {
    let width = ui.available_width();
    let inner_w = width - 28.0;
    let (fill, border, color) = match validation {
        Validation::Empty => (theme::SURFACE, theme::BORDER, theme::TEXT_2),
        Validation::Valid(_) => (theme::rgba(16, 185, 129, 0.1), theme::rgba(16, 185, 129, 0.3), Color32::from_rgb(0xa7, 0xf3, 0xd0)),
        Validation::Invalid(_) => (theme::rgba(244, 63, 94, 0.1), theme::rgba(244, 63, 94, 0.3), Color32::from_rgb(0xfe, 0xcd, 0xd3)),
    };
    let bg = ui.painter().add(Shape::Noop);
    let top = ui.cursor().min;
    let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(top + Vec2::new(14.0, 12.0), Vec2::new(inner_w, 400.0))));
    let line = |t: &str, size: f32, w: Weight, c: Color32| {
        let mut j = theme::job(t, size, w, c);
        j.sections[0].format.line_height = Some(size * 1.5);
        j.wrap.max_width = inner_w;
        j
    };
    match validation {
        Validation::Empty => {
            widgets::text(&mut child, line("Collez ou chargez une configuration WireGuard.", 13.0, Weight::Regular, theme::TEXT_2));
        }
        Validation::Valid(s) => {
            child.horizontal(|ui| {
                icons::paint(ui, Icon::Check, Rect::from_min_size(ui.cursor().min + Vec2::new(0.0, 2.0), Vec2::splat(16.0)), color);
                ui.add_space(24.0);
                widgets::text(ui, line("Configuration valide", 13.0, Weight::Semibold, color));
            });
            child.add_space(4.0);
            let endpoints: Vec<String> = s.peers.iter().map(|p| p.endpoint.clone().unwrap_or_default()).collect();
            let meta = format!(
                "{} → {} · {}",
                s.addresses.join(", "),
                endpoints.join(", "),
                if s.full_tunnel { "tout le trafic" } else { "tunnel partiel" }
            );
            widgets::text(&mut child, line(&meta, 13.0, Weight::Regular, theme::TEXT_2));
            for w in &s.warnings {
                child.add_space(4.0);
                widgets::text(&mut child, line(&format!("•  {w}"), 13.0, Weight::Regular, theme::WARN_TEXT));
            }
        }
        Validation::Invalid(e) => {
            child.horizontal(|ui| {
                icons::paint(ui, Icon::Close, Rect::from_min_size(ui.cursor().min + Vec2::new(0.0, 2.0), Vec2::splat(16.0)), color);
                ui.add_space(24.0);
                ui.vertical(|ui| widgets::text(ui, line(e, 13.0, Weight::Semibold, color)));
            });
        }
    }
    let h = (child.min_rect().height() + 24.0).max(46.0);
    let rect = Rect::from_min_size(top, Vec2::new(width, h));
    ui.painter().set(bg, Shape::Rect(RectShape::new(rect, cr(12.0), fill, Stroke::new(1.0, border), StrokeKind::Inside)));
    ui.allocate_rect(rect, Sense::hover());
}

// ---------- Confirmation de suppression ----------

fn confirm(app: &App, ui: &mut Ui, name: &str, accent: Accent) -> Action {
    let mut action = Action::None;
    let width = ui.available_width();
    ui.add_space(26.0);
    ui.vertical_centered(|ui| {
        let (icon, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
        ui.painter().rect_filled(icon, cr(18.0), theme::rgba(244, 63, 94, 0.14));
        icons::paint_centered(ui, Icon::Trash, icon.center(), 26.0, theme::ROSE);
        ui.add_space(14.0);
        widgets::text(ui, theme::job("Supprimer ce tunnel ?", 19.0, Weight::Bold, theme::TEXT));
        ui.add_space(6.0);
        let mut job = LayoutJob::default();
        let fmt = |w: Weight, c: Color32| {
            let mut f = theme::format(14.0, w, c);
            f.line_height = Some(21.0);
            f
        };
        job.append("Le tunnel ", 0.0, fmt(Weight::Regular, theme::TEXT_2));
        job.append(name, 0.0, fmt(Weight::Bold, theme::TEXT));
        job.append(" et sa clé privée seront définitivement supprimés de ce poste.", 0.0, fmt(Weight::Regular, theme::TEXT_2));
        if app.is_active(name) {
            job.append("\nIl sera d’abord déconnecté.", 0.0, fmt(Weight::Regular, theme::TEXT_2));
        }
        job.wrap.max_width = width - 52.0;
        job.halign = Align::Center;
        widgets::text(ui, job);
        ui.add_space(20.0);
        let total = 100.0 + 10.0 + 118.0;
        let (row, _) = ui.allocate_exact_size(Vec2::new(total, 38.0), Sense::hover());
        let mut child = ui.new_child(UiBuilder::new().max_rect(row).layout(Layout::left_to_right(Align::Center)));
        child.spacing_mut().item_spacing.x = 10.0;
        if Button::new("Annuler").kind(Kind::Ghost).width(100.0).show(&mut child, accent).clicked() {
            action = Action::Close;
        }
        if Button::new("Supprimer").kind(Kind::Danger).width(118.0).show(&mut child, accent).clicked() {
            action = Action::Delete(name.to_string());
        }
    });
    ui.add_space(26.0);
    action
}

// ---------- Options ----------

fn options(app: &App, ui: &mut Ui, accent: Accent) -> Action {
    let mut action = Action::None;
    if header(ui, "Options") {
        action = Action::Close;
    }
    let opts = app.options;
    body(ui, 2000.0, |ui| {
        let boot_desc = if opts.show_tray {
            "Démarre Cryptonaute discrètement dans la zone de notification à l’ouverture de votre session."
        } else {
            "Ouvre la fenêtre de Cryptonaute à l’ouverture de votre session."
        };
        let rows = [
            (
                "tray",
                Icon::Tray,
                (theme::rgba(34, 211, 238, 0.14), theme::CYAN),
                "Icône dans la zone de notification",
                "Accès rapide aux tunnels près de l'horloge. Fermer la fenêtre la masque au lieu de quitter Cryptonaute.",
                opts.show_tray,
            ),
            ("boot", Icon::Boot, (theme::rgba(124, 92, 255, 0.18), theme::VIOLET_LIGHT), "Lancer à l’ouverture de session", boot_desc, opts.autostart),
            (
                "update",
                Icon::Update,
                (theme::rgba(16, 185, 129, 0.16), theme::GREEN),
                "Mises à jour automatiques",
                "Recherche les nouvelles versions sur GitHub au démarrage puis toutes les 6 heures. Rien n’est installé sans votre accord.",
                opts.auto_update,
            ),
        ];
        for (i, (key, icon, colors, title, desc, value)) in rows.into_iter().enumerate() {
            if i > 0 {
                ui.add_space(14.0);
            }
            if let Some(v) = option_row(ui, key, icon, colors, title, desc, value, accent) {
                action = match key {
                    "tray" => Action::ShowTray(v),
                    "boot" => Action::Autostart(v),
                    _ => Action::AutoUpdate(v),
                };
            }
        }
        ui.add_space(14.0);
        let half = (ui.available_width() - 10.0) / 2.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let label = if app.checking_update { "Recherche…" } else { "Rechercher une mise à jour" };
            if Button::new(label).icon(Icon::Refresh).width(half).enabled(!app.checking_update).show(ui, accent).clicked() {
                action = Action::CheckUpdate;
            }
            if Button::new("Patch notes").icon(Icon::Notes).width(half).show(ui, accent).clicked() {
                action = Action::PatchNotes;
            }
        });
        ui.add_space(16.0);
        note(
            ui,
            "Ces réglages sont propres à votre compte et ne nécessitent aucun droit administrateur. Fermer la fenêtre garde vos tunnels actifs ; quitter Cryptonaute les déconnecte.",
        );
    });
    footer(ui, |ui| {
        if Button::new("Terminé").kind(Kind::Primary).show(ui, accent).clicked() {
            action = Action::Close;
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            widgets::text(ui, theme::job(format!("Cryptonaute v{}", env!("CARGO_PKG_VERSION")), 12.0, Weight::Regular, theme::MUTED));
        });
    });
    action
}

/// Ligne d'option cliquable ; renvoie la nouvelle valeur si elle a changé.
#[allow(clippy::too_many_arguments)]
fn option_row(
    ui: &mut Ui,
    key: &str,
    icon: Icon,
    (icon_bg, icon_fg): (Color32, Color32),
    title: &str,
    desc: &str,
    value: bool,
    accent: Accent,
) -> Option<bool> {
    let width = ui.available_width();
    let text_w = width - 16.0 - 42.0 - 14.0 - 14.0 - 38.0 - 16.0;
    let tg = ui.painter().layout_job(theme::job(title, 14.0, Weight::Semibold, theme::TEXT));
    let mut dj = theme::job(desc, 12.5, Weight::Regular, theme::TEXT_2);
    dj.sections[0].format.line_height = Some(12.5 * 1.45);
    dj.wrap.max_width = text_w;
    let dg = ui.painter().layout_job(dj);
    let h = (14.0 * 2.0 + tg.size().y + 3.0 + dg.size().y).max(70.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    let hover = widgets::hover_anim(ui, resp.id, resp.hovered());
    let rect = rect.translate(Vec2::new(0.0, -hover));
    ui.painter().add(RectShape::new(
        rect,
        cr(14.0),
        theme::lerp(theme::SURFACE, theme::SURFACE_2, hover),
        Stroke::new(1.0, theme::lerp(theme::BORDER, theme::BORDER_STRONG, hover)),
        StrokeKind::Inside,
    ));
    let icon_rect = Rect::from_min_size(Pos2::new(rect.left() + 16.0, rect.center().y - 21.0), Vec2::splat(42.0));
    ui.painter().rect_filled(icon_rect, cr(13.0), icon_bg);
    icons::paint_centered(ui, icon, icon_rect.center(), 20.0, icon_fg);
    let x = icon_rect.right() + 14.0;
    let block = tg.size().y + 3.0 + dg.size().y;
    let y = rect.center().y - block / 2.0;
    ui.painter().galley(Pos2::new(x, y), tg.clone(), theme::TEXT);
    ui.painter().galley(Pos2::new(x, y + tg.size().y + 3.0), dg, theme::TEXT_2);
    let sw = Rect::from_min_size(Pos2::new(rect.right() - 16.0 - 38.0, rect.center().y - 11.0), Vec2::new(38.0, 22.0));
    let mut on = value;
    let mut child = ui.new_child(UiBuilder::new().max_rect(sw));
    let switched = widgets::switch(&mut child, Id::new(("option", key)), &mut on, true, accent).changed();
    if switched {
        Some(on)
    } else if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        Some(!value)
    } else {
        None
    }
}

// ---------- Patch notes ----------

fn rich_line(ui: &Ui, text: &str, size: f32, color: Color32, strong: Color32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let lh = Some(size * 1.5);
    for span in changelog::spans(text) {
        let (t, mut f) = match span {
            Span::Plain(t) => (t, theme::format(size, Weight::Regular, color)),
            Span::Bold(t) => (t, theme::format(size, Weight::Semibold, strong)),
            Span::Italic(t) => {
                let mut f = theme::format(size, Weight::Regular, color);
                f.italics = true;
                (t, f)
            }
            Span::Code(t) => {
                let mut f = theme::mono(size - 1.0, strong);
                f.background = theme::SURFACE_2;
                (t, f)
            }
        };
        f.line_height = lh;
        job.append(t, 0.0, f);
    }
    job.wrap.max_width = width;
    let _ = ui;
    job
}

fn notes(ui: &mut Ui, releases: &[changelog::Release], accent: Accent, age: f64, max_h: f32) -> Action {
    let mut action = Action::None;
    if header(ui, "Patch notes") {
        action = Action::Close;
    }
    let current = env!("CARGO_PKG_VERSION");
    let body_h = (ui.ctx().content_rect().height() * 0.62).min(560.0).min(max_h - 68.0 - 70.0);
    body(ui, body_h, |ui| {
        for (i, r) in releases.iter().enumerate() {
            if i > 0 {
                ui.add_space(12.0);
            }
            let delay = (i.min(6) as f64) * 0.04;
            let p = paint::spring(((age - delay) / 0.45).clamp(0.0, 1.0) as f32);
            let is_current = r.version == current;
            let width = ui.available_width();
            let bg = ui.painter().add(Shape::Noop);
            let top = ui.cursor().min + Vec2::new(0.0, 8.0 * (1.0 - p));
            let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(top + Vec2::new(16.0, 14.0), Vec2::new(width - 32.0, 4000.0))));
            child.set_opacity(p.clamp(0.0, 1.0));
            child.horizontal(|ui| {
                widgets::text(ui, theme::job(format!("Version {}", r.version), 15.0, Weight::Bold, theme::TEXT));
                if let Some(d) = r.date.as_deref().and_then(super::format::date_fr) {
                    ui.add_space(10.0);
                    widgets::text(ui, theme::job(d, 12.0, Weight::Regular, theme::MUTED));
                }
                if is_current {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let g = ui.painter().layout_job(theme::job("Version installée", 11.0, Weight::Semibold, theme::MINT));
                        let (b, _) = ui.allocate_exact_size(g.size() + Vec2::new(16.0, 4.0), Sense::hover());
                        ui.painter().rect_filled(b, cr(9.0), theme::rgba(16, 185, 129, 0.18));
                        ui.painter().galley(b.min + Vec2::new(8.0, 2.0), g, theme::MINT);
                    });
                }
            });
            for s in &r.sections {
                if !s.title.is_empty() {
                    child.add_space(12.0);
                    widgets::text(&mut child, theme::caps(&s.title, 11.0, 1.2, theme::CYAN));
                    child.add_space(6.0);
                }
                for (k, item) in s.items.iter().enumerate() {
                    if k > 0 {
                        child.add_space(5.0);
                    }
                    child.horizontal_top(|ui| {
                        ui.add_space(6.0);
                        widgets::text(ui, theme::job("•", 13.0, Weight::Regular, theme::TEXT_2));
                        ui.add_space(8.0);
                        let w = ui.available_width();
                        widgets::text(ui, rich_line(ui, item, 13.0, theme::TEXT_2, theme::TEXT, w));
                    });
                }
            }
            let h = child.min_rect().height() + 28.0;
            let rect = Rect::from_min_size(top, Vec2::new(width, h));
            let shapes = if is_current {
                let brush = paint::linear(rect, 135.0, vec![(0.0, theme::rgba(16, 185, 129, 0.10)), (1.0, theme::rgba(34, 211, 238, 0.05))]);
                vec![
                    paint::gradient_rect(ui.ctx(), rect, 14.0, &brush),
                    Shape::Rect(RectShape::stroke(rect, cr(14.0), Stroke::new(1.0, theme::rgba(16, 185, 129, 0.4)), StrokeKind::Inside)),
                ]
            } else {
                vec![Shape::Rect(RectShape::new(rect, cr(14.0), theme::SURFACE, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside))]
            };
            ui.painter().set(bg, Shape::Vec(shapes).multiply_opacity_all(p.clamp(0.0, 1.0)));
            ui.allocate_rect(Rect::from_min_size(ui.cursor().min, Vec2::new(width, h)), Sense::hover());
        }
    });
    footer(ui, |ui| {
        if Button::new("Fermer").kind(Kind::Primary).show(ui, accent).clicked() {
            action = Action::Close;
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            if Button::new("Toutes les versions sur GitHub").kind(Kind::Ghost).show(ui, accent).clicked() {
                action = Action::OpenPage;
            }
        });
    });
    action
}

/// Opacité appliquée à toutes les formes d'un groupe.
trait GroupOpacity {
    fn multiply_opacity_all(self, opacity: f32) -> Shape;
}

impl GroupOpacity for Shape {
    fn multiply_opacity_all(self, opacity: f32) -> Shape {
        match self {
            Shape::Vec(shapes) => Shape::Vec(shapes.into_iter().map(|s| s.multiply_opacity_all(opacity)).collect()),
            Shape::Mesh(mut mesh) => {
                for v in &mut std::sync::Arc::make_mut(&mut mesh).vertices {
                    v.color = v.color.gamma_multiply(opacity);
                }
                Shape::Mesh(mesh)
            }
            Shape::Rect(mut r) => {
                r.fill = r.fill.gamma_multiply(opacity);
                r.stroke.color = r.stroke.color.gamma_multiply(opacity);
                Shape::Rect(r)
            }
            other => other,
        }
    }
}

// ---------- Mise à jour ----------

fn update(app: &App, ui: &mut Ui, accent: Accent, max_h: f32) -> Action {
    let mut action = Action::None;
    let Some(info) = app.update.clone() else {
        return Action::Close;
    };
    if header(ui, "Mise à jour disponible") && !app.update_ui.installing {
        action = Action::Close;
    }
    body(ui, max_h - 68.0 - 70.0, |ui| {
        // Versions
        ui.horizontal(|ui| {
            let (icon, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
            ui.painter().rect_filled(icon, cr(11.0), theme::rgba(16, 185, 129, 0.16));
            icons::paint_centered(ui, Icon::Update, icon.center(), 17.0, theme::GREEN);
            ui.add_space(12.0);
            widgets::text(ui, theme::job("Cryptonaute ", 15.0, Weight::Semibold, theme::TEXT));
            ui.add_space(6.0);
            widgets::tag(ui, &info.current, theme::SURFACE_2, theme::TEXT);
            ui.add_space(6.0);
            widgets::text(ui, theme::job("→", 15.0, Weight::Semibold, theme::TEXT));
            ui.add_space(6.0);
            widgets::tag(ui, &info.version, theme::rgba(16, 185, 129, 0.18), theme::MINT);
        });
        ui.add_space(14.0);

        // Notes de version
        let width = ui.available_width();
        let bg = ui.painter().add(Shape::Noop);
        let top = ui.cursor().min;
        let mut child = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(top, Vec2::new(width, 220.0))));
        egui::ScrollArea::vertical().id_salt("update-notes").max_height(220.0).auto_shrink([false, true]).show(&mut child, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    let w = width - 28.0;
                    if info.notes.trim().is_empty() {
                        widgets::text(ui, theme::job("Aucune note de version.", 12.5, Weight::Regular, theme::TEXT_2));
                    }
                    for line in info.notes.lines() {
                        let trimmed = line.trim();
                        let (text, heading) = match trimmed.strip_prefix('#') {
                            Some(h) => (h.trim_start_matches('#').trim().to_string(), true),
                            None => (
                                trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).map(|t| format!("• {t}")).unwrap_or_else(|| line.to_string()),
                                false,
                            ),
                        };
                        let job = if heading {
                            theme::job(text, 12.5, Weight::Semibold, theme::TEXT)
                        } else {
                            rich_line(ui, &text, 12.5, theme::TEXT_2, theme::TEXT, w)
                        };
                        widgets::text(ui, job);
                    }
                });
            });
            ui.add_space(12.0);
        });
        let h = child.min_rect().height();
        let rect = Rect::from_min_size(top, Vec2::new(width, h));
        ui.painter().set(bg, Shape::Rect(RectShape::new(rect, cr(12.0), theme::SURFACE, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside)));
        ui.allocate_rect(rect, Sense::hover());
        ui.add_space(14.0);

        let platform = if cfg!(windows) {
            "Windows demandera l’autorisation administrateur, puis Cryptonaute redémarrera tout seul."
        } else if cfg!(target_os = "macos") {
            "Le programme d’installation de macOS s’ouvrira ; relancez Cryptonaute une fois l’installation terminée."
        } else {
            "Votre système demandera le mot de passe administrateur, puis Cryptonaute redémarrera."
        };
        let text = if info.asset.is_some() {
            format!(
                "L’installeur ({}) est téléchargé depuis GitHub et sa signature vérifiée. {platform} Les tunnels actifs seront déconnectés pendant l’installation.",
                super::format::bytes(info.size as f64)
            )
        } else {
            "Aucun installeur automatique n’est disponible pour votre système : téléchargez la nouvelle version depuis GitHub.".to_string()
        };
        note(ui, &text);

        if let Some((received, total)) = app.update_ui.progress.filter(|_| app.update_ui.installing) {
            ui.add_space(14.0);
            let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
            ui.painter().rect_filled(bar, cr(4.0), theme::SURFACE_2);
            let pct = if total > 0 { (received as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
            let shown = ui.ctx().animate_value_with_time(Id::new("update-bar"), pct, 0.25);
            if shown > 0.0 {
                let fill = Rect::from_min_size(bar.min, Vec2::new(bar.width() * shown, bar.height()));
                let brush = paint::linear(bar, 90.0, vec![(0.0, theme::GREEN), (1.0, theme::CYAN)]);
                ui.painter().add(paint::gradient_rect(ui.ctx(), fill, 4.0, &brush));
            }
            ui.add_space(6.0);
            widgets::text(ui, theme::job(app.update_ui.status_text.clone(), 12.0, Weight::Regular, theme::MUTED));
        }
    });
    let installing = app.update_ui.installing;
    footer(ui, |ui| {
        if info.asset.is_some() && Button::new("Installer").kind(Kind::Primary).enabled(!installing).show(ui, accent).clicked() {
            action = Action::InstallUpdate;
        }
        if Button::new("Plus tard").kind(Kind::Ghost).enabled(!installing).show(ui, accent).clicked() {
            action = Action::Close;
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            if Button::new("Voir sur GitHub").kind(Kind::Ghost).show(ui, accent).clicked() {
                action = Action::OpenPage;
            }
        });
    });
    action
}

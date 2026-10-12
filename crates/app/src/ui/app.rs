//! État et logique de l'interface (port de l'ancien `app.js`) : sélection,
//! sondage du service, débits, connexion, éditeur, import, options, mises à jour
//! et notifications. Le dessin est réparti dans les modules voisins.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use cryptonaute_common::config::{self, TunnelSummary, WgConfig};
use cryptonaute_common::ipc::{ServiceStatus, TunnelStatus};
use eframe::egui;

use super::changelog::{self, Release};
use super::format;
use super::theme::Mood;
use crate::backend::{self, Backend, TunnelEntry};
use crate::events::{self, Job};
use crate::{autostart, settings, tray, updater};

/// Opération en cours sur un tunnel.
#[derive(Clone, PartialEq, Eq)]
pub enum PendingKind {
    Connect,
    Disconnect,
}

#[derive(Clone)]
pub struct Pending {
    pub kind: PendingKind,
    pub name: String,
}

/// Débits d'un tunnel actif (octets/s), calculés à partir des compteurs cumulés.
#[derive(Default)]
pub struct Traffic {
    pub since: u64,
    pub prev: Option<(f64, u64, u64)>,
    pub samples: Vec<Sample>,
}

#[derive(Clone, Copy)]
pub struct Sample {
    pub t: f64,
    pub rx: f64,
    pub tx: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

pub struct Toast {
    pub id: u64,
    pub message: String,
    pub kind: ToastKind,
    pub created: f64,
    pub duration: f64,
}

#[derive(Default)]
pub struct ServiceInfo {
    /// `None` tant que le service n'a pas encore été interrogé.
    pub available: Option<bool>,
    pub error: Option<String>,
    pub version: Option<String>,
}

/// Résultat de la validation de l'éditeur.
pub enum Validation {
    Empty,
    Valid(TunnelSummary),
    Invalid(String),
}

pub struct EditorState {
    pub title: String,
    pub name: String,
    pub config: String,
    pub previous: Option<String>,
    pub name_error: Option<String>,
    pub validation: Validation,
    pub saving: bool,
    /// Texte validé en dernier (la validation n'est refaite qu'en cas de changement).
    pub validated: (String, String),
    pub focus_name: bool,
}

pub enum Modal {
    Editor(Box<EditorState>),
    Confirm { name: String },
    Options,
    Notes(Vec<Release>),
    Update,
}

pub struct ModalState {
    pub modal: Modal,
    pub opened: f64,
    pub closing: Option<f64>,
}

#[derive(Clone, Copy, Default)]
pub struct OptionsView {
    pub show_tray: bool,
    pub autostart: bool,
    pub auto_update: bool,
}

#[derive(Default)]
pub struct UpdateUi {
    pub installing: bool,
    pub progress: Option<(u64, u64)>,
    pub status_text: String,
}

pub struct App {
    pub backend: Arc<dyn Backend>,
    jobs: Receiver<Job>,
    pub tunnels: Vec<TunnelEntry>,
    pub selected: Option<String>,
    pub filter: String,
    pub service: ServiceInfo,
    pub status: Option<ServiceStatus>,
    pub pending: Option<Pending>,
    pub last_error: Option<(String, String)>,
    pub traffic: HashMap<String, Traffic>,
    pub toasts: Vec<Toast>,
    next_toast: u64,
    pub modals: Vec<ModalState>,
    pub options: OptionsView,
    pub update: Option<UpdateInfoView>,
    pub update_ui: UpdateUi,
    pub checking_update: bool,
    /// Apparition des éléments de la liste (animation d'entrée).
    pub list_appear: HashMap<String, f64>,
    /// Changement de tunnel affiché (animation d'entrée de la vue).
    pub view_since: f64,
    /// Valeurs qui viennent de changer (effet de pulsation).
    pub pulses: HashMap<&'static str, (String, f64)>,
    /// Heure courante (secondes Unix), relue à chaque passage.
    pub now: f64,
    /// Développement : scène prédéfinie et capture automatique.
    pub scene: Option<super::debug::Scene>,
    /// Développement : faire défiler la vue jusqu'en bas.
    pub scroll_to_bottom: bool,
    /// Plafond de cadence (rendu sans synchronisation verticale).
    pub pacer: super::Pacer,
}

pub type UpdateInfoView = updater::UpdateInfo;

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let mut app = App {
            backend: backend::get(),
            jobs: events::receiver(),
            tunnels: Vec::new(),
            selected: None,
            filter: String::new(),
            service: ServiceInfo::default(),
            status: None,
            pending: None,
            last_error: None,
            traffic: HashMap::new(),
            toasts: Vec::new(),
            next_toast: 0,
            modals: Vec::new(),
            options: OptionsView::default(),
            update: updater::available(),
            update_ui: UpdateUi::default(),
            checking_update: false,
            list_appear: HashMap::new(),
            view_since: 0.0,
            pulses: HashMap::new(),
            now: format::now_secs(),
            scene: super::debug::Scene::from_env(),
            scroll_to_bottom: false,
            pacer: super::Pacer::default(),
        };
        app.refresh_options();
        app.load_tunnels(None);
        spawn_poller(ctx.clone());
        app
    }

    /// Applique les messages des threads d'arrière-plan.
    pub fn drain_jobs(&mut self) {
        while let Ok(job) = self.jobs.try_recv() {
            job(self);
        }
    }

    // ---------- Données ----------

    /// Recharge la liste ; `select` choisit le tunnel à afficher.
    pub fn load_tunnels(&mut self, select: Option<Option<String>>) {
        match self.backend.list() {
            Ok(list) => self.tunnels = list,
            Err(e) => {
                self.toast(e, ToastKind::Error);
                self.tunnels.clear();
            }
        }
        if let Some(sel) = select {
            self.selected = sel;
        }
        let exists = |n: &Option<String>| n.as_ref().is_some_and(|n| self.tunnels.iter().any(|t| &t.name == n));
        if !exists(&self.selected) {
            self.selected = self
                .tunnels
                .iter()
                .find(|t| self.is_active(&t.name))
                .or(self.tunnels.first())
                .map(|t| t.name.clone());
            self.view_since = self.now;
        }
    }

    pub fn current(&self) -> Option<&TunnelEntry> {
        let sel = self.selected.as_ref()?;
        self.tunnels.iter().find(|t| &t.name == sel)
    }

    pub fn active_list(&self) -> &[TunnelStatus] {
        self.status.as_ref().map(|s| s.tunnels.as_slice()).unwrap_or_default()
    }

    pub fn status_of(&self, name: &str) -> Option<&TunnelStatus> {
        self.active_list().iter().find(|s| s.name == name)
    }

    pub fn is_active(&self, name: &str) -> bool {
        self.status_of(name).is_some()
    }

    /// État affiché pour le tunnel sélectionné (pilote les couleurs d'accent).
    pub fn mood(&self) -> Mood {
        let Some(t) = self.current() else { return Mood::Disconnected };
        if let Some(p) = &self.pending {
            if p.name == t.name {
                return if p.kind == PendingKind::Connect { Mood::Connecting } else { Mood::Disconnecting };
            }
        }
        if self.is_active(&t.name) {
            Mood::Connected
        } else if self.last_error.as_ref().is_some_and(|(n, _)| n == &t.name) {
            Mood::Error
        } else {
            Mood::Disconnected
        }
    }

    pub fn select(&mut self, name: &str) {
        if self.selected.as_deref() != Some(name) {
            self.selected = Some(name.to_string());
            self.view_since = self.now;
        }
    }

    pub fn select_relative(&mut self, delta: i32) {
        let i = self.tunnels.iter().position(|t| Some(&t.name) == self.selected.as_ref());
        let next = match i {
            Some(i) => (i as i32 + delta).clamp(0, self.tunnels.len() as i32 - 1) as usize,
            None => 0,
        };
        if let Some(name) = self.tunnels.get(next).map(|t| t.name.clone()) {
            self.select(&name);
        }
    }

    // ---------- Service & sondage ----------

    fn on_poll(&mut self, view: Result<ServiceStatus, String>) {
        let was = self.service.available;
        let prev_active: Vec<String> = self.active_list().iter().map(|s| s.name.clone()).collect();
        match view {
            Ok(status) => {
                self.service.available = Some(true);
                self.service.error = None;
                if self.pending.is_none() {
                    self.status = Some(status);
                }
                if self.service.version.is_none() {
                    let backend = self.backend.clone();
                    events::spawn(move || backend.version(), |app, v| app.service.version = v.ok());
                }
            }
            Err(e) => {
                self.service.available = Some(false);
                self.service.error = Some(e);
                if self.pending.is_none() {
                    self.status = None;
                }
            }
        }
        let _ = was;
        self.sample_traffic();
        if self.pending.is_none() {
            let gone: Vec<String> = prev_active.into_iter().filter(|n| !self.is_active(n)).collect();
            for name in gone {
                self.toast(format!("Tunnel « {name} » déconnecté"), ToastKind::Info);
            }
        }
    }

    /// Débits instantanés de chaque tunnel actif.
    pub fn sample_traffic(&mut self) {
        let t = format::now_secs();
        let active: Vec<TunnelStatus> = self.active_list().to_vec();
        for st in &active {
            let tr = self.traffic.entry(st.name.clone()).or_default();
            if tr.since != st.connected_since {
                *tr = Traffic {
                    since: st.connected_since,
                    ..Traffic::default()
                };
            }
            let sample = match tr.prev {
                Some((pt, prx, ptx)) if t > pt => {
                    let dt = t - pt;
                    Sample {
                        t,
                        rx: (st.rx_bytes.saturating_sub(prx)) as f64 / dt,
                        tx: (st.tx_bytes.saturating_sub(ptx)) as f64 / dt,
                    }
                }
                _ => Sample { t, rx: 0.0, tx: 0.0 },
            };
            tr.samples.push(sample);
            tr.prev = Some((t, st.rx_bytes, st.tx_bytes));
            tr.samples.retain(|s| t - s.t < 75.0);
        }
        self.traffic.retain(|name, _| active.iter().any(|s| &s.name == name));
    }

    pub fn samples_of(&self, name: &str) -> &[Sample] {
        self.traffic.get(name).map(|t| t.samples.as_slice()).unwrap_or_default()
    }

    pub fn last_rate(&self, name: &str) -> (f64, f64) {
        self.samples_of(name).last().map(|s| (s.rx, s.tx)).unwrap_or_default()
    }

    // ---------- Connexion ----------

    pub fn toggle_power(&mut self) {
        let Some(t) = self.current() else { return };
        if t.error.is_some() || self.pending.is_some() || self.service.available == Some(false) {
            return;
        }
        let name = t.name.clone();
        let disconnecting = self.is_active(&name);
        let before: Vec<String> = self.active_list().iter().map(|s| s.name.clone()).collect();
        self.pending = Some(Pending {
            kind: if disconnecting { PendingKind::Disconnect } else { PendingKind::Connect },
            name: name.clone(),
        });
        self.last_error = None;
        let backend = self.backend.clone();
        let target = name.clone();
        events::spawn(
            move || {
                if disconnecting {
                    backend.disconnect(Some(&target))
                } else {
                    backend.connect(&target)
                }
            },
            move |app, result| {
                app.pending = None;
                match result {
                    Ok(status) => {
                        app.status = Some(status);
                        app.traffic.remove(&name);
                        app.sample_traffic();
                        app.toast(
                            if disconnecting { format!("Déconnecté de « {name} »") } else { format!("Connecté à « {name} »") },
                            ToastKind::Success,
                        );
                        // Un tunnel complet en remplace un autre : on le signale.
                        let replaced: Vec<String> =
                            before.into_iter().filter(|n| n != &name && !app.is_active(n)).collect();
                        if !replaced.is_empty() {
                            app.toast_for(
                                format!("{} déconnecté : un seul tunnel complet à la fois", quote_list(&replaced)),
                                ToastKind::Info,
                                5.5,
                            );
                        }
                    }
                    Err(e) => {
                        if !disconnecting {
                            app.last_error = Some((name.clone(), e.clone()));
                        }
                        app.toast_for(e, ToastKind::Error, 6.5);
                    }
                }
            },
        );
    }

    // ---------- Notifications ----------

    pub fn toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toast_for(message, kind, 4.2);
    }

    pub fn toast_for(&mut self, message: impl Into<String>, kind: ToastKind, secs: f64) {
        self.next_toast += 1;
        self.toasts.push(Toast {
            id: self.next_toast,
            message: message.into(),
            kind,
            created: self.now,
            duration: secs,
        });
    }

    // ---------- Modales ----------

    pub fn open_modal(&mut self, modal: Modal) {
        self.modals.push(ModalState {
            modal,
            opened: self.now,
            closing: None,
        });
    }

    /// Ferme la modale du dessus (avec animation).
    pub fn close_top_modal(&mut self) {
        if let Some(m) = self.modals.iter_mut().rev().find(|m| m.closing.is_none()) {
            if matches!(m.modal, Modal::Update) && self.update_ui.installing {
                return;
            }
            m.closing = Some(self.now);
        }
    }

    pub fn close_modal_where(&mut self, pred: impl Fn(&Modal) -> bool) {
        let now = self.now;
        for m in self.modals.iter_mut().filter(|m| m.closing.is_none() && pred(&m.modal)) {
            m.closing = Some(now);
        }
    }

    pub fn has_modal(&self) -> bool {
        self.modals.iter().any(|m| m.closing.is_none())
    }

    pub fn has_modal_of(&self, pred: impl Fn(&Modal) -> bool) -> bool {
        self.modals.iter().any(|m| m.closing.is_none() && pred(&m.modal))
    }

    // ---------- Éditeur ----------

    fn open_editor(&mut self, title: String, name: String, config: String, previous: Option<String>) {
        let focus_name = name.is_empty();
        let mut editor = EditorState {
            title,
            name,
            config,
            previous,
            name_error: None,
            validation: Validation::Empty,
            saving: false,
            validated: (String::from("\u{0}"), String::new()),
            focus_name,
        };
        self.validate_editor(&mut editor);
        self.open_modal(Modal::Editor(Box::new(editor)));
    }

    pub fn new_tunnel(&mut self) {
        let name = self.unique_name("wg", &[]);
        self.open_editor("Nouveau tunnel".into(), name, config::new_template(), None);
    }

    pub fn edit_tunnel(&mut self) {
        let Some(name) = self.current().map(|t| t.name.clone()) else { return };
        match self.backend.load(&name) {
            Ok(config) => self.open_editor(format!("Modifier « {name} »"), name.clone(), config, Some(name)),
            Err(e) => self.toast(e, ToastKind::Error),
        }
    }

    /// Valide le nom et la configuration (si modifiés depuis la dernière fois).
    pub fn validate_editor(&self, ed: &mut EditorState) {
        if ed.validated.0 == ed.name && ed.validated.1 == ed.config {
            return;
        }
        ed.validated = (ed.name.clone(), ed.config.clone());
        let name = ed.name.trim();
        ed.name_error = match config::validate_tunnel_name(name) {
            Err(e) => (!name.is_empty()).then_some(e),
            Ok(()) => {
                let renamed = ed.previous.as_ref().is_none_or(|p| !p.eq_ignore_ascii_case(name));
                (renamed && self.tunnels.iter().any(|t| t.name.eq_ignore_ascii_case(name)))
                    .then(|| "un tunnel porte déjà ce nom".to_string())
            }
        };
        ed.validation = if ed.config.trim().is_empty() {
            Validation::Empty
        } else {
            match WgConfig::parse(&ed.config) {
                Ok(cfg) => Validation::Valid(cfg.summary()),
                Err(e) => Validation::Invalid(e.to_string()),
            }
        };
    }

    pub fn editor_can_save(ed: &EditorState) -> bool {
        let name = ed.name.trim();
        !ed.saving
            && matches!(ed.validation, Validation::Valid(_))
            && ed.name_error.is_none()
            && config::validate_tunnel_name(name).is_ok()
    }

    pub fn save_editor(&mut self) {
        let Some(ed) = self.modals.iter_mut().rev().find_map(|m| match &mut m.modal {
            Modal::Editor(ed) if m.closing.is_none() => Some(ed),
            _ => None,
        }) else {
            return;
        };
        if !Self::editor_can_save(ed) {
            return;
        }
        let name = ed.name.trim().to_string();
        let previous = ed.previous.clone();
        let result = self.backend.save(&name, &ed.config, previous.as_deref());
        match result {
            Ok(()) => {
                let was_active = previous.as_ref().is_some_and(|p| self.is_active(p));
                self.close_modal_where(|m| matches!(m, Modal::Editor(_)));
                self.toast(
                    if previous.is_some() { format!("« {name} » mis à jour") } else { format!("Tunnel « {name} » ajouté") },
                    ToastKind::Success,
                );
                if was_active {
                    self.toast_for("Reconnectez le tunnel pour appliquer les modifications", ToastKind::Info, 5.5);
                }
                self.load_tunnels(Some(Some(name)));
                self.view_since = self.now;
            }
            Err(e) => self.toast(e, ToastKind::Error),
        }
    }

    /// Nouvelle paire de clés dans la configuration en cours d'édition.
    pub fn regenerate_keys(ed: &mut EditorState) -> String {
        let template = config::new_template();
        let key = template.lines().find_map(|l| l.strip_prefix("PrivateKey = ")).unwrap_or_default().to_string();
        let public = template.lines().find_map(|l| l.strip_prefix("# Clé publique : ")).unwrap_or_default().to_string();
        let has_key = ed.config.lines().any(|l| l.trim_start().to_ascii_lowercase().starts_with("privatekey"));
        if has_key {
            ed.config = ed
                .config
                .lines()
                .map(|l| {
                    let t = l.trim_start();
                    if t.to_ascii_lowercase().starts_with("privatekey") && t.contains('=') {
                        format!("{}= {key}", &l[..l.find('=').unwrap_or(l.len())])
                    } else if t.starts_with('#') && t.contains("Clé publique :") {
                        format!("# Clé publique : {public}")
                    } else {
                        l.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !ed.config.ends_with('\n') {
                ed.config.push('\n');
            }
        } else {
            ed.config = template;
        }
        public
    }

    // ---------- Import ----------

    pub fn unique_name(&self, base: &str, reserved: &[String]) -> String {
        let taken = |n: &str| {
            self.tunnels.iter().any(|t| t.name.eq_ignore_ascii_case(n)) || reserved.iter().any(|r| r.eq_ignore_ascii_case(n))
        };
        if !taken(base) {
            return base.to_string();
        }
        let stem: String = base.chars().take(29).collect();
        (2..).map(|i| format!("{stem}-{i}")).find(|n| !taken(n)).unwrap_or_default()
    }

    fn name_from_file(&self, path: &std::path::Path, reserved: &[String]) -> String {
        let stem = path.file_name().and_then(|f| f.to_str()).unwrap_or("tunnel");
        let lower = stem.to_ascii_lowercase();
        let stem = if lower.ends_with(".conf") || lower.ends_with(".txt") { &stem[..stem.rfind('.').unwrap_or(stem.len())] } else { stem };
        let cleaned: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || "_=+.-".contains(c) { c } else { '-' })
            .collect();
        let cleaned: String = cleaned.trim_start_matches(['.', '-']).chars().take(32).collect();
        let cleaned = cleaned.trim_end_matches('.');
        self.unique_name(if cleaned.is_empty() { "tunnel" } else { cleaned }, reserved)
    }

    /// Importe des fichiers : un seul s'ouvre dans l'éditeur, plusieurs sont enregistrés.
    pub fn import_files(&mut self, paths: Vec<PathBuf>) {
        let files: Vec<(PathBuf, String)> = paths
            .into_iter()
            .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() < 64 * 1024))
            .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t)))
            .collect();
        match files.len() {
            0 => {}
            1 => {
                let (path, text) = files.into_iter().next().unwrap_or_default();
                let name = self.name_from_file(&path, &[]);
                self.open_editor("Importer une configuration".into(), name, text, None);
            }
            _ => {
                let mut added: Vec<String> = Vec::new();
                for (path, text) in files {
                    let name = self.name_from_file(&path, &added);
                    match self.backend.save(&name, &text, None) {
                        Ok(()) => added.push(name),
                        Err(e) => {
                            let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                            self.toast_for(format!("{file} : {e}"), ToastKind::Error, 6.0);
                        }
                    }
                }
                let n = added.len();
                if n > 0 {
                    let s = if n > 1 { "s" } else { "" };
                    self.toast(format!("{n} tunnel{s} importé{s}"), ToastKind::Success);
                }
                let select = added.last().cloned().or(self.selected.clone());
                self.load_tunnels(Some(select));
            }
        }
    }

    pub fn pick_import_files(&mut self) {
        events::spawn(
            || {
                rfd::FileDialog::new()
                    .add_filter("Configuration WireGuard", &["conf", "txt"])
                    .set_title("Importer des configurations")
                    .pick_files()
                    .unwrap_or_default()
            },
            |app, files| app.import_files(files),
        );
    }

    /// « Charger un fichier » dans l'éditeur ouvert.
    pub fn pick_editor_file(&mut self) {
        events::spawn(
            || {
                rfd::FileDialog::new()
                    .add_filter("Configuration WireGuard", &["conf", "txt"])
                    .set_title("Charger une configuration")
                    .pick_file()
            },
            |app, file| {
                let Some(path) = file else { return };
                let Ok(text) = std::fs::read_to_string(&path) else {
                    app.toast("Lecture du fichier impossible", ToastKind::Error);
                    return;
                };
                let name = app.name_from_file(&path, &[]);
                if let Some(ed) = app.editor_mut() {
                    ed.config = text;
                    if ed.name.trim().is_empty() || ed.previous.is_none() {
                        ed.name = name;
                    }
                }
            },
        );
    }

    pub fn editor_mut(&mut self) -> Option<&mut EditorState> {
        self.modals.iter_mut().rev().find_map(|m| match &mut m.modal {
            Modal::Editor(ed) if m.closing.is_none() => Some(ed.as_mut()),
            _ => None,
        })
    }

    // ---------- Suppression ----------

    pub fn ask_delete(&mut self) {
        if let Some(name) = self.current().map(|t| t.name.clone()) {
            self.open_modal(Modal::Confirm { name });
        }
    }

    pub fn confirm_delete(&mut self, name: String) {
        self.close_modal_where(|m| matches!(m, Modal::Confirm { .. }));
        let backend = self.backend.clone();
        let active = self.is_active(&name);
        events::spawn(
            move || {
                let status = if active { Some(backend.disconnect(Some(&name))) } else { None };
                (name.clone(), status, backend.delete(&name))
            },
            |app, (name, status, deleted)| {
                if let Some(Ok(s)) = status {
                    app.status = Some(s);
                }
                match deleted {
                    Ok(()) => {
                        app.toast(format!("« {name} » supprimé"), ToastKind::Success);
                        app.selected = None;
                        app.load_tunnels(None);
                    }
                    Err(e) => app.toast(e, ToastKind::Error),
                }
            },
        );
    }

    // ---------- Options ----------

    pub fn refresh_options(&mut self) {
        self.options = OptionsView {
            show_tray: tray::is_enabled(),
            autostart: autostart::is_enabled(),
            auto_update: settings::load().auto_update,
        };
    }

    pub fn open_options(&mut self) {
        if !self.has_modal_of(|m| matches!(m, Modal::Options)) {
            self.refresh_options();
            self.open_modal(Modal::Options);
        }
    }

    pub fn set_show_tray(&mut self, visible: bool) {
        let mut prefs = settings::load();
        prefs.show_tray = visible;
        match settings::save(&prefs) {
            Ok(()) => {
                tray::set_visible(visible);
                self.toast(
                    if visible {
                        "Icône affichée dans la zone de notification"
                    } else {
                        "Icône retirée : fermer la fenêtre quittera Cryptonaute"
                    },
                    ToastKind::Success,
                );
            }
            Err(e) => self.toast(e, ToastKind::Error),
        }
        self.refresh_options();
    }

    pub fn set_autostart(&mut self, enabled: bool) {
        match autostart::set_enabled(enabled) {
            Ok(()) => self.toast(
                if enabled { "Cryptonaute démarrera à l’ouverture de votre session" } else { "Lancement au démarrage désactivé" },
                ToastKind::Success,
            ),
            Err(e) => self.toast(e, ToastKind::Error),
        }
        self.refresh_options();
    }

    pub fn set_auto_update(&mut self, enabled: bool) {
        let mut prefs = settings::load();
        prefs.auto_update = enabled;
        match settings::save(&prefs) {
            Ok(()) => self.toast(
                if enabled {
                    "Cryptonaute recherchera automatiquement les mises à jour"
                } else {
                    "Recherche automatique des mises à jour désactivée"
                },
                ToastKind::Success,
            ),
            Err(e) => self.toast(e, ToastKind::Error),
        }
        self.refresh_options();
    }

    // ---------- Patch notes ----------

    pub fn open_patch_notes(&mut self) {
        let releases = changelog::parse(include_str!("../../../../CHANGELOG.md"));
        self.open_modal(Modal::Notes(releases));
    }

    // ---------- Mises à jour ----------

    /// Une mise à jour a été trouvée (recherche automatique ou manuelle).
    pub fn on_update_found(&mut self, info: Option<updater::UpdateInfo>, announce: bool) {
        let is_new = info.as_ref().map(|i| &i.version) != self.update.as_ref().map(|i| &i.version);
        self.update = info;
        if let (true, true, Some(i)) = (announce, is_new, &self.update) {
            let version = i.version.clone();
            self.toast_for(format!("Cryptonaute {version} est disponible"), ToastKind::Info, 6.0);
        }
    }

    pub fn open_update(&mut self) {
        if self.update.is_some() && !self.has_modal_of(|m| matches!(m, Modal::Update)) {
            self.update_ui = UpdateUi::default();
            self.open_modal(Modal::Update);
        }
    }

    pub fn check_update_now(&mut self) {
        if self.checking_update {
            return;
        }
        self.checking_update = true;
        events::spawn(updater::check, |app, result| {
            app.checking_update = false;
            match result {
                Ok(found) => {
                    updater::store(found.clone());
                    app.on_update_found(found.clone(), false);
                    if found.is_some() {
                        app.close_modal_where(|m| matches!(m, Modal::Options | Modal::Notes(_)));
                        app.open_update();
                    } else {
                        app.toast("Cryptonaute est à jour", ToastKind::Success);
                    }
                }
                Err(e) => app.toast_for(e, ToastKind::Error, 6.0),
            }
        });
    }

    pub fn install_update(&mut self) {
        let Some(info) = self.update.clone() else { return };
        if info.asset.is_none() || self.update_ui.installing {
            return;
        }
        self.update_ui = UpdateUi {
            installing: true,
            progress: Some((0, info.size)),
            status_text: "Téléchargement de l’installeur…".into(),
        };
        std::thread::spawn(move || {
            let mut last = 0;
            let result = updater::download(&info, |received, total| {
                // Une notification tous les 256 Kio suffit à animer la barre.
                if received - last >= 256 * 1024 || received == total {
                    last = received;
                    events::post(move |app| {
                        app.update_ui.progress = Some((received, total));
                        app.update_ui.status_text = if received >= total {
                            "Vérification de la signature et lancement de l’installeur…".into()
                        } else {
                            format!("Téléchargement : {} sur {}", format::bytes(received as f64), format::bytes(total as f64))
                        };
                    });
                }
            })
            .and_then(|path| updater::launch(&path));
            events::post(move |app| match result {
                Ok(after) => {
                    app.update_ui.status_text = "Installeur lancé…".into();
                    match after {
                        updater::AfterLaunch::Exit => crate::request_quit(),
                        updater::AfterLaunch::Restart => crate::restart(),
                    }
                }
                Err(e) => {
                    app.update_ui = UpdateUi::default();
                    app.toast_for(e, ToastKind::Error, 7.0);
                }
            });
        });
    }
}

/// « « usa » », « « usa », « infra » ».
pub fn quote_list(names: &[String]) -> String {
    names.iter().map(|n| format!("« {n} »")).collect::<Vec<_>>().join(", ")
}

/// Interroge le service chaque seconde.
fn spawn_poller(ctx: egui::Context) {
    let backend = backend::get();
    std::thread::spawn(move || loop {
        let view = backend.status();
        events::post(move |app| app.on_poll(view));
        ctx.request_repaint();
        std::thread::sleep(std::time::Duration::from_secs(1));
    });
}

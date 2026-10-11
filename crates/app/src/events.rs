//! Canal entre les threads d'arrière-plan (service, icône de notification,
//! mises à jour) et l'interface : chaque message est une fonction appliquée à
//! l'état de l'application, puis l'interface est redessinée.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use eframe::egui;

use crate::ui::App;

pub type Job = Box<dyn FnOnce(&mut App) + Send>;

struct Channel {
    tx: Mutex<Sender<Job>>,
    ctx: Mutex<Option<egui::Context>>,
}

static CHANNEL: OnceLock<Channel> = OnceLock::new();
static RECEIVER: Mutex<Option<Receiver<Job>>> = Mutex::new(None);

fn channel_ref() -> &'static Channel {
    CHANNEL.get_or_init(|| {
        let (tx, rx) = channel();
        *RECEIVER.lock().unwrap_or_else(|e| e.into_inner()) = Some(rx);
        Channel {
            tx: Mutex::new(tx),
            ctx: Mutex::new(None),
        }
    })
}

/// Récepteur des messages, réservé à l'interface (appelé une fois).
pub fn receiver() -> Receiver<Job> {
    channel_ref();
    RECEIVER.lock().unwrap_or_else(|e| e.into_inner()).take().expect("récepteur déjà pris")
}

/// Contexte egui, pour redessiner l'interface depuis un autre thread.
pub fn set_context(ctx: egui::Context) {
    *channel_ref().ctx.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx);
}

pub fn context() -> Option<egui::Context> {
    channel_ref().ctx.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Applique `job` à l'état de l'interface (au prochain passage) et la réveille.
pub fn post(job: impl FnOnce(&mut App) + Send + 'static) {
    let channel = channel_ref();
    let _ = channel.tx.lock().unwrap_or_else(|e| e.into_inner()).send(Box::new(job));
    wake();
}

/// Réveille l'interface, même fenêtre masquée.
pub fn wake() {
    if let Some(ctx) = context() {
        ctx.request_repaint();
    }
}

/// Exécute `work` sur un thread, puis applique son résultat à l'interface.
pub fn spawn<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut App, T) + Send + 'static,
) {
    std::thread::spawn(move || {
        let result = work();
        post(move |app| done(app, result));
    });
}

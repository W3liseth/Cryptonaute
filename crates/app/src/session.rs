//! Présence de l'application auprès du service.
//!
//! Tant que Cryptonaute tourne (fenêtre ouverte ou dans la zone de
//! notification), une session reste ouverte avec le service. Quitter
//! l'application la ferme : le service déconnecte alors les tunnels si aucune
//! autre application n'est ouverte. Si l'application s'arrête brutalement
//! (plantage, fin de session Windows), la connexion se rompt et le service fait
//! de même après un court délai. Le service, lui, reste démarré.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::client::Session;

/// Intervalle des signes de vie, bien inférieur au délai d'expiration du service.
const KEEPALIVE: Duration = Duration::from_secs(5);

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static CLOSING: AtomicBool = AtomicBool::new(false);

fn lock() -> std::sync::MutexGuard<'static, Option<Session>> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ouvre la session et la maintient (reconnexion si le service redémarre).
pub fn spawn() {
    std::thread::spawn(|| loop {
        {
            let mut session = lock();
            if CLOSING.load(Ordering::SeqCst) {
                return;
            }
            let alive = session.as_ref().is_some_and(|s| s.keepalive().is_ok());
            if !alive {
                *session = Session::open().ok();
            }
        }
        std::thread::sleep(KEEPALIVE);
    });
}

/// Ferme la session à la sortie de l'application ; rend la main une fois les
/// tunnels déconnectés (si c'était la dernière application ouverte).
pub fn close() {
    CLOSING.store(true, Ordering::SeqCst);
    if let Some(session) = lock().take() {
        if let Err(e) = session.close() {
            eprintln!("fermeture de la session : {e}");
        }
    }
}

//! Service privilégié de Cryptonaute (Windows, Linux, macOS).
//!
//! Installé une fois avec les droits administrateur, il s'exécute en tant que
//! SYSTEM (service Windows) ou root (systemd / launchd) et expose un canal IPC
//! local permettant aux utilisateurs d'activer ou de désactiver un tunnel
//! WireGuard sans élévation.

mod ipc;
mod logger;
mod manager;

#[cfg(windows)]
#[path = "windows/mod.rs"]
mod platform;

#[cfg(unix)]
#[path = "unix/mod.rs"]
mod platform;

use std::process::ExitCode;

#[cfg(windows)]
const USAGE: &str = "Utilisation :\n  \
    cryptonaute-service install     installe et démarre le service (administrateur)\n  \
    cryptonaute-service uninstall   arrête et supprime le service (administrateur)\n  \
    cryptonaute-service console     exécute le service au premier plan (débogage)";

#[cfg(unix)]
const USAGE: &str = "Utilisation :\n  \
    cryptonaute-service install     installe et démarre le démon (root)\n  \
    cryptonaute-service uninstall   arrête et supprime le démon (root)\n  \
    cryptonaute-service run         exécute le démon au premier plan (root)";

fn main() -> ExitCode {
    let arg = std::env::args().nth(1);
    let result = match arg.as_deref() {
        Some("install") => platform::install::install(),
        Some("uninstall") => platform::install::uninstall(),
        #[cfg(windows)]
        Some("--service") => platform::service::run_as_service(),
        #[cfg(windows)]
        Some("console") => {
            logger::init(true);
            platform::service::run_console()
        }
        #[cfg(unix)]
        Some("run") => {
            logger::init(true);
            platform::run()
        }
        _ => {
            eprintln!("Cryptonaute service {}\n\n{USAGE}", env!("CARGO_PKG_VERSION"));
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e:#}");
            eprintln!("Erreur : {e:#}");
            ExitCode::FAILURE
        }
    }
}

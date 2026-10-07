//! Implémentation Linux / macOS : démon root (systemd ou launchd), socket Unix
//! et WireGuard via `defguard_wireguard_rs`.

#[cfg(target_os = "linux")]
mod firewall;
pub mod install;
mod server;
pub mod tunnel;

use std::sync::Arc;

pub use tunnel::TunnelManager;

/// Exécute le démon au premier plan jusqu'à SIGTERM ou SIGINT, puis ferme le tunnel.
pub fn run() -> anyhow::Result<()> {
    let manager = Arc::new(TunnelManager::new());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    log::info!("démarrage du démon Cryptonaute {}", env!("CARGO_PKG_VERSION"));
    // Règles laissées par une exécution précédente interrompue.
    #[cfg(target_os = "linux")]
    firewall::remove();
    let result = runtime.block_on(async {
        let shutdown = async {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = signal(SignalKind::terminate()).expect("gestion de SIGTERM");
            tokio::select! {
                _ = term.recv() => {},
                _ = tokio::signal::ctrl_c() => {},
            }
        };
        server::serve(Arc::clone(&manager), shutdown).await
    });
    manager.disconnect();
    log::info!("démon arrêté");
    result
}

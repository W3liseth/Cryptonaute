//! Intégration avec le gestionnaire de services Windows (SCM).

use std::ffi::OsString;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle};
use windows_service::{define_windows_service, service_dispatcher};

use super::tunnel::{TunnelManager, WireGuardNt};
use super::pipe;
use crate::logger;

pub const SERVICE_NAME: &str = "Cryptonaute";
pub const SERVICE_DISPLAY_NAME: &str = "Cryptonaute VPN";

define_windows_service!(ffi_service_main, service_main);

pub fn run_as_service() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .context("impossible de se connecter au gestionnaire de services (ce programme doit être lancé en tant que service)")
}

fn service_main(_args: Vec<OsString>) {
    logger::init(false);
    log::info!("démarrage du service Cryptonaute {}", env!("CARGO_PKG_VERSION"));
    if let Err(e) = run_service() {
        log::error!("arrêt du service sur erreur : {e:#}");
    }
}

fn set_state(handle: &ServiceStatusHandle, state: ServiceState, code: u32) -> anyhow::Result<()> {
    let controls_accepted = if state == ServiceState::Running {
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
    } else {
        ServiceControlAccept::empty()
    };
    handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted,
        exit_code: ServiceExitCode::Win32(code),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    })?;
    Ok(())
}

fn run_service() -> anyhow::Result<()> {
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
    let handler = move |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = stop_tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let handle = service_control_handler::register(SERVICE_NAME, handler)?;
    set_state(&handle, ServiceState::Running, 0)?;

    let result = run_core(async move {
        let _ = stop_rx.changed().await;
    });

    if let Err(e) = &result {
        log::error!("{e:#}");
    }
    set_state(&handle, ServiceState::StopPending, 0)?;
    set_state(&handle, ServiceState::Stopped, u32::from(result.is_err()))?;
    log::info!("service arrêté");
    result
}

pub fn run_console() -> anyhow::Result<()> {
    log::info!("mode console : Ctrl+C pour arrêter");
    run_core(async {
        let _ = tokio::signal::ctrl_c().await;
    })
}

/// Boucle principale : serveur IPC jusqu'à l'arrêt, puis fermeture propre des tunnels actifs.
fn run_core(shutdown: impl Future<Output = ()> + Send + 'static) -> anyhow::Result<()> {
    let manager = Arc::new(TunnelManager::new(WireGuardNt::new(logger::exe_dir().join("wireguard.dll"))));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(pipe::serve(Arc::clone(&manager), shutdown));
    manager.disconnect(None);
    result
}

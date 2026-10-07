//! Installation / désinstallation du service (nécessite les droits administrateur).

use std::ffi::OsString;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceDependency, ServiceErrorControl,
    ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType, ServiceState,
    ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use super::service::{SERVICE_DISPLAY_NAME, SERVICE_NAME};

pub fn install() -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .context("accès au gestionnaire de services refusé (lancez cette commande en administrateur)")?;

    let exe = std::env::current_exe()?;
    if !exe.with_file_name("wireguard.dll").exists() {
        anyhow::bail!(
            "wireguard.dll introuvable à côté de {} (voir scripts/fetch-wireguard-nt.ps1)",
            exe.display()
        );
    }

    let info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments: vec![OsString::from("--service")],
        dependencies: vec![
            ServiceDependency::Service(OsString::from("Nsi")),
            ServiceDependency::Service(OsString::from("TcpIp")),
        ],
        account_name: None, // LocalSystem
        account_password: None,
    };

    let service = manager
        .create_service(
            &info,
            ServiceAccess::CHANGE_CONFIG | ServiceAccess::START | ServiceAccess::QUERY_STATUS,
        )
        .context("création du service impossible (déjà installé ?)")?;
    service.set_description(
        "Gère les tunnels WireGuard de Cryptonaute pour les utilisateurs sans droits administrateur.",
    )?;
    let restart = ServiceAction {
        action_type: ServiceActionType::Restart,
        delay: Duration::from_secs(3),
    };
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86_400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![restart.clone(), restart.clone(), restart]),
    })?;
    service.start::<&str>(&[]).context("démarrage du service impossible")?;
    println!("Service « {SERVICE_DISPLAY_NAME} » installé et démarré.");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("accès au gestionnaire de services refusé (lancez cette commande en administrateur)")?;
    let service = manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
        )
        .context("service introuvable ou accès refusé")?;

    if service.query_status()?.current_state != ServiceState::Stopped {
        let _ = service.stop();
        let deadline = Instant::now() + Duration::from_secs(15);
        while service.query_status()?.current_state != ServiceState::Stopped {
            if Instant::now() > deadline {
                anyhow::bail!("le service ne s'est pas arrêté à temps");
            }
            thread::sleep(Duration::from_millis(250));
        }
    }
    service.delete()?;
    println!("Service « {SERVICE_DISPLAY_NAME} » supprimé.");
    Ok(())
}

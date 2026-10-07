//! Serveur IPC sur named pipe.
//!
//! Sécurité :
//! - propriétaire SYSTEM (`O:SY`) : le client vérifie ce propriétaire, ce qui
//!   empêche un processus non privilégié d'usurper le pipe ;
//! - DACL : contrôle total pour SYSTEM et les administrateurs, lecture/écriture
//!   pour les utilisateurs connectés en interactif (`IU`) uniquement ;
//! - connexions distantes refusées (taille des messages et inactivité : voir `crate::ipc`).

use std::ffi::c_void;
use std::future::Future;
use std::sync::Arc;

use anyhow::Context;
use cryptonaute_common::ipc::PIPE_NAME;
use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

use super::tunnel::TunnelManager;

const PIPE_SDDL: &str = "O:SYD:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

// SAFETY: le descripteur est immuable après création et libéré une seule fois.
unsafe impl Send for SecurityDescriptor {}
unsafe impl Sync for SecurityDescriptor {}

impl SecurityDescriptor {
    fn from_sddl(sddl: &str) -> std::io::Result<Self> {
        let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: chaîne UTF-16 terminée par un zéro ; `sd` reçoit une allocation LocalAlloc.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(SecurityDescriptor(sd))
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: alloué par ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe { LocalFree(self.0 as HLOCAL) };
    }
}

fn create_pipe(sd: &SecurityDescriptor, first: bool) -> std::io::Result<NamedPipeServer> {
    let mut attrs = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    // SAFETY: `attrs` et le descripteur restent valides pendant l'appel.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .pipe_mode(PipeMode::Byte)
            .max_instances(16)
            .create_with_security_attributes_raw(PIPE_NAME, &mut attrs as *mut _ as *mut c_void)
    }
}

pub async fn serve(
    manager: Arc<TunnelManager>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let sd = SecurityDescriptor::from_sddl(PIPE_SDDL).context("descripteur de sécurité")?;
    // `first_pipe_instance` échoue si un autre processus a déjà créé ce pipe.
    let mut server = create_pipe(&sd, true).context("création du named pipe (déjà utilisé ?)")?;
    log::info!("à l'écoute sur {PIPE_NAME}");

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            res = server.connect() => {
                let next = create_pipe(&sd, false).context("création d'une instance de pipe")?;
                let client = std::mem::replace(&mut server, next);
                if let Err(e) = res {
                    log::warn!("connexion IPC échouée : {e}");
                    continue;
                }
                let manager = Arc::clone(&manager);
                tokio::spawn(async move {
                    if let Err(e) = crate::ipc::handle_client(client, manager).await {
                        log::warn!("client IPC : {e}");
                    }
                });
            }
        }
    }
    Ok(())
}

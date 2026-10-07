//! Serveur IPC sur socket Unix.
//!
//! Sécurité :
//! - le démon s'exécute en root et crée le socket dans un dossier qui lui
//!   appartient (0755) : un utilisateur ne peut pas y placer son propre socket ;
//! - le client vérifie en plus que le processus à l'autre bout est root ;
//! - le socket est accessible en lecture/écriture à tous les utilisateurs locaux
//!   (équivalent des « utilisateurs interactifs » sous Windows).

use std::fs;
use std::future::Future;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use cryptonaute_common::ipc::SOCKET_PATH;
use tokio::net::UnixListener;

use super::tunnel::TunnelManager;

pub async fn serve(
    manager: Arc<TunnelManager>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    // SAFETY: appel système sans effet de bord.
    if unsafe { libc::geteuid() } != 0 {
        bail!("le démon Cryptonaute doit être exécuté en tant que root");
    }

    let socket = Path::new(SOCKET_PATH);
    let dir = socket.parent().expect("le chemin du socket a un dossier parent");
    fs::create_dir_all(dir).with_context(|| format!("création de {}", dir.display()))?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755))?;

    // Socket laissé par une exécution précédente interrompue.
    if let Ok(meta) = fs::symlink_metadata(socket) {
        if !meta.file_type().is_socket() {
            bail!("{SOCKET_PATH} existe et n'est pas un socket");
        }
        fs::remove_file(socket)?;
    }

    let listener = UnixListener::bind(socket).with_context(|| format!("écoute sur {SOCKET_PATH}"))?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o666))?;
    log::info!("à l'écoute sur {SOCKET_PATH}");

    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            res = listener.accept() => {
                let (stream, _) = match res {
                    Ok(conn) => conn,
                    Err(e) => {
                        log::warn!("connexion IPC échouée : {e}");
                        continue;
                    }
                };
                let manager = Arc::clone(&manager);
                tokio::spawn(async move {
                    if let Err(e) = crate::ipc::handle_client(stream, manager).await {
                        log::warn!("client IPC : {e}");
                    }
                });
            }
        }
    }
    let _ = fs::remove_file(socket);
    Ok(())
}

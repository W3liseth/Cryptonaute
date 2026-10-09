//! Traitement des requêtes IPC, commun à toutes les plateformes.
//!
//! Le transport (named pipe sous Windows, socket Unix ailleurs) est fourni par
//! le module de la plateforme ; ce module lit une requête JSON par ligne, borne
//! la taille des messages, applique un délai d'inactivité et répond.
//!
//! Sessions : chaque application ouverte garde une connexion « attachée »
//! (`Attach`). Quand la dernière se termine, les tunnels sont déconnectés : le
//! service, lui, reste démarré pour les prochaines connexions (il est nécessaire
//! aux utilisateurs sans droits administrateur et ne fait rien sans tunnel).

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cryptonaute_common::ipc::{encode, Request, Response, MAX_MESSAGE_LEN, PROTOCOL_VERSION};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::platform::TunnelManager;

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// Une session attachée envoie `Hello` toutes les quelques secondes ; au-delà
/// de ce délai sans nouvelle, elle est considérée comme perdue.
const SESSION_TIMEOUT: Duration = Duration::from_secs(60);
/// Délai de grâce après la perte (et non la fermeture) de la dernière session :
/// une application relancée aussitôt (plantage, mise à jour) retrouve ses tunnels.
const LOST_SESSION_GRACE: Duration = Duration::from_secs(15);

/// Sessions d'application ouvertes, tous utilisateurs confondus.
static ATTACHED: AtomicUsize = AtomicUsize::new(0);
/// Incrémenté à chaque ouverture de session, pour annuler un délai de grâce en cours.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub async fn handle_client<S>(stream: S, manager: Arc<TunnelManager>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    let mut attached = false;
    let result = loop {
        line.clear();
        let limited = (&mut reader).take(MAX_MESSAGE_LEN as u64);
        let timeout = if attached { SESSION_TIMEOUT } else { IDLE_TIMEOUT };
        let n = match tokio::time::timeout(timeout, read_line(limited, &mut line)).await {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => break Err(e),
            Err(_) => break Ok(()), // inactif
        };
        if n == 0 {
            break Ok(());
        }
        if !line.ends_with('\n') {
            let resp = Response::Error {
                message: "message trop volumineux".into(),
            };
            let _ = write_half.write_all(&encode(&resp)).await;
            break Ok(());
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(Request::Attach) if !attached => {
                attached = true;
                ATTACHED.fetch_add(1, Ordering::SeqCst);
                GENERATION.fetch_add(1, Ordering::SeqCst);
                Response::Session
            }
            Ok(Request::Detach) if attached => {
                end_session(&manager, true).await;
                // La réponse part une fois les tunnels déconnectés.
                write_half.write_all(&encode(&Response::Session)).await?;
                write_half.flush().await?;
                return Ok(());
            }
            Ok(Request::Attach | Request::Detach) => Response::Error {
                message: "requête de session inattendue".into(),
            },
            Ok(req) => dispatch(req, &manager).await,
            Err(_) => Response::Error {
                message: "requête invalide".into(),
            },
        };
        let written = async {
            write_half.write_all(&encode(&response)).await?;
            write_half.flush().await
        };
        if let Err(e) = written.await {
            break Err(e);
        }
    };
    if attached {
        end_session(&manager, false).await;
    }
    result
}

/// Fin d'une session : si c'était la dernière, déconnecte tous les tunnels,
/// aussitôt (`closed`) ou après le délai de grâce (session perdue).
async fn end_session(manager: &Arc<TunnelManager>, closed: bool) {
    if ATTACHED.fetch_sub(1, Ordering::SeqCst) > 1 {
        return;
    }
    if !closed {
        let generation = GENERATION.load(Ordering::SeqCst);
        tokio::time::sleep(LOST_SESSION_GRACE).await;
        if ATTACHED.load(Ordering::SeqCst) > 0 || GENERATION.load(Ordering::SeqCst) != generation {
            return; // une application s'est reconnectée entre-temps
        }
    }
    let m = Arc::clone(manager);
    let _ = tokio::task::spawn_blocking(move || {
        if !m.status().tunnels.is_empty() {
            log::info!(
                "dernière application {} : déconnexion des tunnels",
                if closed { "quittée" } else { "perdue" }
            );
            m.disconnect(None);
        }
    })
    .await;
}

async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(mut r: R, buf: &mut String) -> std::io::Result<usize> {
    r.read_line(buf).await
}

async fn dispatch(req: Request, manager: &Arc<TunnelManager>) -> Response {
    let m = Arc::clone(manager);
    let joined = match req {
        Request::Hello => {
            return Response::Hello {
                protocol: PROTOCOL_VERSION,
                version: env!("CARGO_PKG_VERSION").to_string(),
            }
        }
        Request::Status => tokio::task::spawn_blocking(move || Ok(m.status())).await,
        Request::Disconnect { name } => {
            tokio::task::spawn_blocking(move || Ok(m.disconnect(name.as_deref()))).await
        }
        Request::Connect { name, config } => {
            tokio::task::spawn_blocking(move || m.connect(&name, &config)).await
        }
        Request::Attach | Request::Detach => unreachable!("traité par handle_client"),
    };
    match joined {
        Ok(Ok(status)) => Response::Status(status),
        Ok(Err(message)) => Response::Error { message },
        Err(e) => Response::Error {
            message: format!("erreur interne du service : {e}"),
        },
    }
}

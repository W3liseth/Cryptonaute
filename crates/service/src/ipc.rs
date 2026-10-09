//! Traitement des requêtes IPC, commun à toutes les plateformes.
//!
//! Le transport (named pipe sous Windows, socket Unix ailleurs) est fourni par
//! le module de la plateforme ; ce module lit une requête JSON par ligne, borne
//! la taille des messages, applique un délai d'inactivité et répond.

use std::sync::Arc;
use std::time::Duration;

use cryptonaute_common::ipc::{encode, Request, Response, MAX_MESSAGE_LEN, PROTOCOL_VERSION};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::platform::TunnelManager;

const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn handle_client<S>(stream: S, manager: Arc<TunnelManager>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    loop {
        line.clear();
        let limited = (&mut reader).take(MAX_MESSAGE_LEN as u64);
        let n = match tokio::time::timeout(IDLE_TIMEOUT, read_line(limited, &mut line)).await {
            Ok(r) => r?,
            Err(_) => return Ok(()), // inactif
        };
        if n == 0 {
            return Ok(());
        }
        if !line.ends_with('\n') {
            let resp = Response::Error {
                message: "message trop volumineux".into(),
            };
            write_half.write_all(&encode(&resp)).await?;
            return Ok(());
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(req, &manager).await,
            Err(_) => Response::Error {
                message: "requête invalide".into(),
            },
        };
        write_half.write_all(&encode(&response)).await?;
        write_half.flush().await?;
    }
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
    };
    match joined {
        Ok(Ok(status)) => Response::Status(status),
        Ok(Err(message)) => Response::Error { message },
        Err(e) => Response::Error {
            message: format!("erreur interne du service : {e}"),
        },
    }
}

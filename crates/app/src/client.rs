//! Client IPC vers le service Cryptonaute.
//!
//! Avant d'envoyer une requête (qui peut contenir une clé privée), on vérifie
//! l'identité du serveur : le named pipe doit appartenir à SYSTEM (Windows),
//! le processus à l'autre bout du socket Unix doit être root (Linux, macOS).
//! Un processus non privilégié ne peut donc pas se faire passer pour le service.

use std::io::{BufRead, BufReader, Read, Write};

use cryptonaute_common::ipc::{encode, Request, Response, MAX_MESSAGE_LEN};

#[derive(Debug)]
pub enum ClientError {
    /// Le service n'est pas installé ou pas démarré.
    Unavailable,
    Other(String),
}

#[cfg(windows)]
const UNAVAILABLE_HINT: &str =
    "Réinstallez l'application ou démarrez le service « Cryptonaute VPN ».";
#[cfg(target_os = "macos")]
const UNAVAILABLE_HINT: &str =
    "Réinstallez l'application ou lancez « sudo launchctl kickstart system/fr.cryptonaute.service ».";
#[cfg(all(unix, not(target_os = "macos")))]
const UNAVAILABLE_HINT: &str =
    "Réinstallez l'application ou lancez « sudo systemctl start cryptonaute ».";

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Unavailable => write!(f, "Le service Cryptonaute n'est pas démarré. {UNAVAILABLE_HINT}"),
            ClientError::Other(msg) => f.write_str(msg),
        }
    }
}

fn other(e: impl std::fmt::Display) -> ClientError {
    ClientError::Other(format!("communication avec le service : {e}"))
}

#[cfg(windows)]
mod transport {
    use std::fs::{File, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::time::Duration;

    use cryptonaute_common::ipc::PIPE_NAME;
    use windows_sys::Win32::Foundation::{
        LocalFree, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, HANDLE, HLOCAL,
    };
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
    use windows_sys::Win32::Security::{
        IsWellKnownSid, WinLocalSystemSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    };
    use windows_sys::Win32::Storage::FileSystem::SECURITY_IDENTIFICATION;

    use super::{other, ClientError};

    pub type Stream = File;

    pub fn connect() -> Result<Stream, ClientError> {
        for _ in 0..40 {
            // SECURITY_IDENTIFICATION : le serveur peut identifier le client mais pas l'usurper.
            match OpenOptions::new()
                .read(true)
                .write(true)
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(PIPE_NAME)
            {
                Ok(f) => return Ok(f),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) if e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) => {
                    return Err(ClientError::Unavailable)
                }
                Err(e) => return Err(other(e)),
            }
        }
        Err(ClientError::Other("le service est occupé, réessayez".into()))
    }

    /// Le pipe doit appartenir à SYSTEM.
    pub fn verify_server(pipe: &Stream) -> Result<(), ClientError> {
        let mut owner: PSID = std::ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: handle valide ; `sd` est alloué par le système et libéré ci-dessous,
        // `owner` pointe à l'intérieur de `sd`.
        unsafe {
            let err = GetSecurityInfo(
                pipe.as_raw_handle() as HANDLE,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut sd,
            );
            if err != 0 {
                return Err(other(std::io::Error::from_raw_os_error(err as i32)));
            }
            let is_system = IsWellKnownSid(owner, WinLocalSystemSid) != 0;
            LocalFree(sd as HLOCAL);
            if !is_system {
                return Err(ClientError::Other(
                    "le pipe du service n'appartient pas à SYSTEM : connexion refusée par sécurité".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
mod transport {
    use std::io::ErrorKind;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    use cryptonaute_common::ipc::SOCKET_PATH;

    use super::{other, ClientError};

    pub type Stream = UnixStream;

    pub fn connect() -> Result<Stream, ClientError> {
        let stream = UnixStream::connect(SOCKET_PATH).map_err(|e| match e.kind() {
            ErrorKind::NotFound | ErrorKind::ConnectionRefused => ClientError::Unavailable,
            _ => other(e),
        })?;
        // L'activation d'un tunnel (résolution DNS comprise) peut prendre quelques secondes.
        stream.set_read_timeout(Some(Duration::from_secs(60))).map_err(other)?;
        Ok(stream)
    }

    #[cfg(target_os = "linux")]
    fn peer_uid(stream: &Stream) -> std::io::Result<u32> {
        let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: tampon de la taille attendue par SO_PEERCRED.
        let rc = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut cred as *mut _ as *mut libc::c_void,
                &mut len,
            )
        };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(cred.uid)
    }

    #[cfg(not(target_os = "linux"))]
    fn peer_uid(stream: &Stream) -> std::io::Result<u32> {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: descripteur de socket valide, pointeurs vers des entiers locaux.
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(uid)
    }

    /// Le processus à l'autre bout du socket doit être root.
    pub fn verify_server(stream: &Stream) -> Result<(), ClientError> {
        match peer_uid(stream).map_err(other)? {
            0 => Ok(()),
            uid => Err(ClientError::Other(format!(
                "le socket du service est tenu par l'utilisateur {uid} et non par root : connexion refusée par sécurité"
            ))),
        }
    }
}

pub fn request(req: &Request) -> Result<Response, ClientError> {
    let stream = transport::connect()?;
    transport::verify_server(&stream)?;
    let mut writer = &stream;
    writer.write_all(&encode(req)).map_err(other)?;
    writer.flush().map_err(other)?;

    let mut line = String::new();
    BufReader::new(&stream)
        .take(MAX_MESSAGE_LEN as u64)
        .read_line(&mut line)
        .map_err(other)?;
    if !line.ends_with('\n') {
        return Err(other("réponse tronquée"));
    }
    serde_json::from_str(&line).map_err(other)
}

//! Instance unique : un second lancement réaffiche simplement la fenêtre de la
//! première instance (socket local par utilisateur), puis se termine.

use std::io::{BufRead, BufReader, Write};

use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Stream};

/// Nom du socket, propre à l'utilisateur (et distinct en développement).
fn socket_name() -> String {
    #[cfg(windows)]
    let user = std::env::var("USERNAME").unwrap_or_default();
    #[cfg(unix)]
    // SAFETY: getuid ne peut pas échouer.
    let user = unsafe { libc::getuid() }.to_string();
    let suffix = if cfg!(debug_assertions) { "-dev" } else { "" };
    format!("cryptonaute-ui-{user}{suffix}.sock")
}

/// Devient l'instance principale, ou demande à celle qui tourne déjà de se
/// montrer. Renvoie `false` si l'application doit se terminer.
pub fn acquire() -> bool {
    let Ok(name) = socket_name().to_ns_name::<GenericNamespaced>() else {
        return true; // pas de socket local : on ne bloque pas le lancement
    };
    if let Ok(mut stream) = Stream::connect(name.clone()) {
        let _ = stream.write_all(b"show\n");
        return false;
    }
    match ListenerOptions::new().name(name).create_sync() {
        Ok(listener) => {
            std::thread::spawn(move || {
                for conn in listener.incoming().flatten() {
                    let mut line = String::new();
                    let _ = BufReader::new(conn).read_line(&mut line);
                    crate::show_window();
                }
            });
            true
        }
        Err(e) => {
            eprintln!("instance unique indisponible : {e}");
            true
        }
    }
}

/// Après une mise à jour (Linux), la nouvelle instance attend la fin de l'ancienne.
pub fn wait_for_previous() {
    let Some(pid) = std::env::var("CRYPTONAUTE_WAIT_PID").ok().and_then(|p| p.parse::<u32>().ok()) else {
        return;
    };
    for _ in 0..100 {
        if !process_alive(pid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // SAFETY: le signal 0 ne fait que tester l'existence du processus.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    // SAFETY: handle ouvert puis refermé ; attente nulle.
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return false;
        }
        let alive = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        alive
    }
}

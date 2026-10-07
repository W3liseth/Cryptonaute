//! Code partagé entre le service privilégié (`cryptonaute-service`) et
//! l'interface graphique non privilégiée (`cryptonaute`).

pub mod config;
pub mod ipc;

pub use config::{ConfigError, Key, TunnelSummary, WgConfig};

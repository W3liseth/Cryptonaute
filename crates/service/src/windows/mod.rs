//! Implémentation Windows : service SCM, WireGuardNT et named pipe.

pub mod install;
mod net;
mod pipe;
pub mod service;
pub mod tunnel;

pub use tunnel::TunnelManager;

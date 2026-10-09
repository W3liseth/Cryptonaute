//! Tunnels actifs et règles de coexistence, communs à toutes les plateformes.
//!
//! Plusieurs tunnels peuvent être actifs en même temps, chacun sur sa propre
//! interface. Le routage repose sur la correspondance du plus long préfixe : les
//! réseaux d'un tunnel partiel (ex. 10.8.0.0/24) l'emportent sur la route par
//! défaut d'un tunnel complet, qui garde tout le reste du trafic. D'où les règles :
//! - un seul tunnel complet à la fois (en activer un autre remplace le premier) ;
//! - deux tunnels ne peuvent pas router des réseaux qui se chevauchent, ni
//!   partager une adresse : le résultat serait ambigu.
//!
//! La création des interfaces est déléguée au [`Backend`] de la plateforme.

use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use cryptonaute_common::config::{validate_tunnel_name, WgConfig};
use cryptonaute_common::ipc::{ServiceStatus, TunnelStatus, MAX_ACTIVE_TUNNELS};

/// Compteurs d'un tunnel actif.
#[derive(Default)]
pub struct Stats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub last_handshake: Option<u64>,
    pub endpoint: Option<String>,
}

/// Un tunnel actif : sa configuration et l'interface créée par la plateforme.
pub struct Active<T> {
    pub name: String,
    pub cfg: WgConfig,
    pub since: SystemTime,
    pub tunnel: T,
}

/// Création et suppression des interfaces WireGuard d'une plateforme.
pub trait Backend {
    type Tunnel: Send;

    /// Crée et configure l'interface du tunnel `name`, à côté des tunnels `others`
    /// déjà actifs. En cas d'échec, plus rien ne doit subsister de l'interface.
    fn up(&self, name: &str, cfg: &WgConfig, others: &[Active<Self::Tunnel>]) -> anyhow::Result<Self::Tunnel>;

    /// Supprime l'interface du tunnel, avec ses routes et son DNS. Les routes
    /// encore utiles aux tunnels `remaining` (même serveur) sont conservées.
    fn down(&self, tunnel: Self::Tunnel, cfg: &WgConfig, remaining: &[Active<Self::Tunnel>]);

    fn stats(&self, tunnel: &Self::Tunnel) -> Stats;

    /// Appelé après chaque changement de l'ensemble des tunnels actifs.
    fn changed(&self, _active: &[Active<Self::Tunnel>]) {}
}

struct Inner<T> {
    active: Vec<Active<T>>,
    last_error: Option<String>,
}

pub struct Manager<B: Backend> {
    backend: B,
    inner: Mutex<Inner<B::Tunnel>>,
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl<B: Backend> Manager<B> {
    pub fn new(backend: B) -> Self {
        Manager {
            backend,
            inner: Mutex::new(Inner {
                active: Vec::new(),
                last_error: None,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner<B::Tunnel>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> ServiceStatus {
        let inner = self.lock();
        ServiceStatus {
            tunnels: inner
                .active
                .iter()
                .map(|a| {
                    let stats = self.backend.stats(&a.tunnel);
                    TunnelStatus {
                        name: a.name.clone(),
                        full_tunnel: a.cfg.is_full_tunnel(),
                        connected_since: unix_secs(a.since),
                        rx_bytes: stats.rx_bytes,
                        tx_bytes: stats.tx_bytes,
                        last_handshake: stats.last_handshake,
                        endpoint: stats.endpoint,
                    }
                })
                .collect(),
            last_error: inner.last_error.clone(),
        }
    }

    pub fn connect(&self, name: &str, config_text: &str) -> Result<ServiceStatus, String> {
        validate_tunnel_name(name)?;
        // Le service revalide toujours la configuration : le client n'est pas digne de confiance.
        let cfg = WgConfig::parse(config_text).map_err(|e| e.to_string())?;
        let full = cfg.is_full_tunnel();

        let mut inner = self.lock();
        // Tunnels remplacés : celui du même nom (reconnexion) et, pour un tunnel
        // complet, l'éventuel autre tunnel complet.
        let replaced = |a: &Active<B::Tunnel>| {
            a.name.eq_ignore_ascii_case(name) || (full && a.cfg.is_full_tunnel())
        };
        let kept: Vec<&Active<B::Tunnel>> = inner.active.iter().filter(|a| !replaced(a)).collect();
        let refusal = kept
            .iter()
            .find_map(|a| {
                cfg.conflict_with(&a.cfg).map(|why| {
                    format!("« {name} » ne peut pas être actif en même temps que « {} » : {why}", a.name)
                })
            })
            .or_else(|| {
                (kept.len() >= MAX_ACTIVE_TUNNELS)
                    .then(|| format!("{MAX_ACTIVE_TUNNELS} tunnels au plus peuvent être actifs en même temps"))
            });
        if let Some(msg) = refusal {
            log::warn!("activation de « {name} » refusée : {msg}");
            inner.last_error = Some(msg.clone());
            return Err(msg);
        }

        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut inner.active).into_iter().partition(replaced);
        inner.active = kept;
        for a in gone.iter().rev() {
            if !a.name.eq_ignore_ascii_case(name) {
                log::info!("« {} » remplacé par « {name} » (un seul tunnel complet à la fois)", a.name);
            }
        }
        let had_replacements = !gone.is_empty();
        for a in gone {
            self.down(a, &inner.active);
        }
        inner.last_error = None;

        match self.backend.up(name, &cfg, &inner.active) {
            Ok(tunnel) => {
                log::info!(
                    "tunnel « {name} » actif ({}, {} tunnel(s) actif(s))",
                    if full { "complet" } else { "partiel" },
                    inner.active.len() + 1
                );
                inner.active.push(Active {
                    name: name.to_string(),
                    cfg,
                    since: SystemTime::now(),
                    tunnel,
                });
                self.backend.changed(&inner.active);
                drop(inner);
                Ok(self.status())
            }
            Err(e) => {
                let msg = format!("{e:#}");
                log::error!("échec de l'activation de « {name} » : {msg}");
                inner.last_error = Some(msg.clone());
                if had_replacements {
                    self.backend.changed(&inner.active);
                }
                Err(msg)
            }
        }
    }

    /// Arrête le tunnel `name`, ou tous les tunnels. Sans effet sur un tunnel inactif.
    pub fn disconnect(&self, name: Option<&str>) -> ServiceStatus {
        let mut inner = self.lock();
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut inner.active)
            .into_iter()
            .partition(|a| name.is_none_or(|n| a.name.eq_ignore_ascii_case(n)));
        inner.active = kept;
        if !gone.is_empty() {
            for a in gone.into_iter().rev() {
                self.down(a, &inner.active);
            }
            self.backend.changed(&inner.active);
        }
        drop(inner);
        self.status()
    }

    fn down(&self, a: Active<B::Tunnel>, remaining: &[Active<B::Tunnel>]) {
        self.backend.down(a.tunnel, &a.cfg, remaining);
        log::info!("tunnel « {} » arrêté", a.name);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Backend factice : journalise les créations et suppressions d'interfaces.
    #[derive(Default)]
    struct Fake {
        log: Mutex<Vec<String>>,
    }

    impl Backend for Fake {
        type Tunnel = String;

        fn up(&self, name: &str, _cfg: &WgConfig, _others: &[Active<String>]) -> anyhow::Result<String> {
            if name == "panne" {
                anyhow::bail!("échec simulé");
            }
            self.log.lock().unwrap().push(format!("+{name}"));
            Ok(name.to_string())
        }

        fn down(&self, tunnel: String, _cfg: &WgConfig, _remaining: &[Active<String>]) {
            self.log.lock().unwrap().push(format!("-{tunnel}"));
        }

        fn stats(&self, _tunnel: &String) -> Stats {
            Stats::default()
        }
    }

    fn config(address: &str, allowed: &str) -> String {
        format!(
            "[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = {address}\n\
             [Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\n\
             Endpoint = 198.51.100.1:51820\nAllowedIPs = {allowed}\n"
        )
    }

    fn names(status: &ServiceStatus) -> Vec<&str> {
        status.tunnels.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn full_and_split_tunnels_coexist() {
        let m = Manager::new(Fake::default());
        let usa = config("10.2.0.2/32", "0.0.0.0/0, ::/0");
        let infra = config("10.8.0.2/32", "10.8.0.0/24");

        m.connect("usa", &usa).unwrap();
        let st = m.connect("infra", &infra).unwrap();
        assert_eq!(names(&st), ["usa", "infra"]);
        assert!(st.tunnel("usa").unwrap().full_tunnel);
        assert!(!st.tunnel("infra").unwrap().full_tunnel);

        // Un second tunnel complet remplace le premier, le partiel reste actif.
        let st = m.connect("japon", &config("10.3.0.2/32", "0.0.0.0/0")).unwrap();
        assert_eq!(names(&st), ["infra", "japon"]);

        // Réseaux qui se chevauchent : refus, sans toucher aux tunnels actifs.
        let err = m.connect("lab", &config("10.9.0.2/32", "10.0.0.0/8")).unwrap_err();
        assert!(err.contains("« infra »"), "{err}");
        assert_eq!(names(&m.status()), ["infra", "japon"]);
        assert!(m.status().last_error.is_some());

        // Reconnexion d'un tunnel actif (même nom, casse comprise) : remplacé, pas refusé.
        let st = m.connect("INFRA", &infra).unwrap();
        assert_eq!(names(&st), ["japon", "INFRA"]);
        assert!(st.last_error.is_none());

        assert_eq!(names(&m.disconnect(Some("japon"))), ["INFRA"]);
        assert!(m.disconnect(None).tunnels.is_empty());
        assert_eq!(
            *m.backend.log.lock().unwrap(),
            ["+usa", "+infra", "-usa", "+japon", "-infra", "+INFRA", "-japon", "-INFRA"]
        );
    }

    #[test]
    fn failed_connection_keeps_other_tunnels() {
        let m = Manager::new(Fake::default());
        m.connect("usa", &config("10.2.0.2/32", "0.0.0.0/0")).unwrap();
        assert!(m.connect("panne", &config("10.8.0.2/32", "10.8.0.0/24")).is_err());
        let st = m.status();
        assert_eq!(names(&st), ["usa"]);
        assert!(st.last_error.unwrap().contains("échec simulé"));
    }

    #[test]
    fn active_tunnels_are_limited() {
        let m = Manager::new(Fake::default());
        for i in 0..MAX_ACTIVE_TUNNELS {
            m.connect(&format!("t{i}"), &config(&format!("10.{i}.0.2/32"), &format!("10.{i}.0.0/24")))
                .unwrap();
        }
        let err = m.connect("trop", &config("10.99.0.2/32", "10.99.0.0/24")).unwrap_err();
        assert!(err.contains("au plus"), "{err}");
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
//! Update-Ablauf: Prüfen, nach Zustimmung laden, nach der Spiegelung installieren.
//! Netzwerkzugriffe laufen in Hintergrund-Threads; dieser Zustand lebt im UI-Thread.
use crate::events::{self, Event};
use crate::i18n::{tr, trf};
use crate::signed_source::SignedHttpSource;
use crate::update_policy::UpdatePolicy;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager};

pub const UPDATE_URL: &str = "https://pcmirror.oel.best";
const PERIOD: Duration = Duration::from_secs(6 * 60 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(45);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const IDLE_GRACE: Duration = Duration::from_secs(2);

pub enum UpdateEvent {
    Checked(Result<Option<Box<UpdateInfo>>, String>),
    Progress(i16),
    Downloaded(Result<(), String>),
}

/// Was die Oberfläche als Nächstes tun soll.
#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    /// Empfänger stoppen, danach `apply_after_receiver_stopped` aufrufen.
    PrepareInstall,
    /// Velopack übernimmt; die App muss sich jetzt beenden.
    Quit,
}

pub struct UpdateService {
    manager: Option<UpdateManager>,
    policy: UpdatePolicy,
    generation: u64,
    busy: bool,
    manual: bool,
    banner: bool,
    automatic: bool,
    info: Option<Box<UpdateInfo>>,
    status: &'static str,
    progress: Option<i16>,
    deadline: Option<Instant>,
    idle_grace: Option<Instant>,
    next_check: Option<Instant>,
}

impl UpdateService {
    pub fn new() -> Self {
        let manager = match UpdateManager::new(SignedHttpSource::new(UPDATE_URL), None, None) {
            Ok(manager) => Some(manager),
            Err(error) => {
                log::info!("Updates deaktiviert (nicht installiert): {error}");
                None
            }
        };
        Self {
            manager,
            policy: UpdatePolicy::default(),
            generation: 0,
            busy: false,
            manual: false,
            banner: false,
            automatic: true,
            info: None,
            status: "",
            progress: None,
            deadline: None,
            idle_grace: None,
            next_check: None,
        }
    }

    pub fn start(&mut self, automatic: bool) {
        self.set_automatic(automatic);
        if automatic {
            self.next_check = Some(Instant::now() + Duration::from_secs(2));
        }
    }

    pub fn set_automatic(&mut self, enabled: bool) {
        self.automatic = enabled;
        self.next_check = enabled.then(|| Instant::now() + PERIOD);
    }

    pub fn check(&mut self, manual: bool) {
        if self.busy || self.policy.preparing {
            return;
        }
        if self.policy.downloaded {
            self.banner = true;
            return;
        }
        let Some(manager) = self.manager.clone() else {
            if manual {
                self.status = "Updates sind nur in der installierten Version verfügbar.";
            }
            return;
        };
        self.busy = true;
        self.manual = manual;
        self.status = "Nach Updates suchen …";
        self.generation += 1;
        self.deadline = Some(Instant::now() + CHECK_TIMEOUT);
        let generation = self.generation;
        std::thread::spawn(move || {
            let result = match manager.check_for_updates() {
                Ok(UpdateCheck::UpdateAvailable(info)) => Ok(Some(info)),
                Ok(_) => Ok(None),
                Err(error) => Err(error.to_string()),
            };
            events::post(Event::Update {
                generation,
                event: UpdateEvent::Checked(result),
            });
        });
    }

    pub fn install(&mut self) {
        if self.busy || self.info.is_none() || self.policy.preparing {
            return;
        }
        self.banner = true;
        self.policy.requested = true;
        if self.policy.downloaded {
            self.maybe_prepare();
            return;
        }
        let (Some(manager), Some(info)) = (self.manager.clone(), self.info.clone()) else {
            return;
        };
        self.busy = true;
        self.progress = Some(0);
        self.generation += 1;
        self.deadline = Some(Instant::now() + DOWNLOAD_TIMEOUT);
        let generation = self.generation;
        std::thread::spawn(move || {
            let (sender, receiver) = mpsc::channel::<i16>();
            let forwarder = std::thread::spawn(move || {
                for value in receiver {
                    events::post(Event::Update {
                        generation,
                        event: UpdateEvent::Progress(value),
                    });
                }
            });
            // Velopack prüft Größe und SHA-256 aus dem signierten Feed vor der Installation.
            let result = manager
                .download_updates(&info, Some(sender))
                .map_err(|e| e.to_string());
            let _ = forwarder.join();
            events::post(Event::Update {
                generation,
                event: UpdateEvent::Downloaded(result),
            });
        });
    }

    pub fn postpone(&mut self) {
        self.policy.postpone();
        self.idle_grace = None;
        self.banner = false;
    }

    pub fn set_streaming(&mut self, streaming: bool) {
        self.policy.streaming = streaming;
        if streaming {
            self.idle_grace = None;
        } else {
            self.maybe_prepare();
        }
    }

    fn maybe_prepare(&mut self) {
        if self.policy.can_prepare() && self.idle_grace.is_none() {
            self.idle_grace = Some(Instant::now() + IDLE_GRACE);
        }
    }

    /// Regelmäßig aus dem UI-Timer aufrufen.
    pub fn tick(&mut self) -> Action {
        let now = Instant::now();
        if self.deadline.is_some_and(|d| now > d) {
            self.deadline = None;
            let installing =
                self.policy.requested || self.policy.preparing || self.progress.is_some();
            log::warn!("Update-Vorgang hat das Zeitlimit erreicht; App läuft weiter.");
            self.generation += 1; // späte Ergebnisse ignorieren
            self.fail(
                if installing {
                    "Update-Zeitlimit erreicht. Später erneut versuchen."
                } else {
                    ""
                },
                installing,
            );
        }
        if self.next_check.is_some_and(|t| now > t) {
            self.next_check = self.automatic.then(|| now + PERIOD);
            self.check(false);
        }
        if self.idle_grace.is_some_and(|t| now > t) {
            self.idle_grace = None;
            if self.policy.can_prepare() {
                self.policy.preparing = true;
                self.status = "Update wird installiert …";
                return Action::PrepareInstall;
            }
        }
        Action::None
    }

    pub fn handle(&mut self, generation: u64, event: UpdateEvent) -> Action {
        if generation != self.generation {
            return Action::None;
        }
        match event {
            UpdateEvent::Checked(result) => {
                self.busy = false;
                self.deadline = None;
                match result {
                    Ok(info) => {
                        self.status = if info.is_some() {
                            "Update verfügbar (v%1)"
                        } else {
                            "Die App ist aktuell."
                        };
                        self.banner = info.is_some();
                        self.info = info;
                    }
                    Err(error) => {
                        log::warn!("Update-Prüfung ohne Ergebnis: {error}");
                        // Automatische Prüfungen bleiben still, manuelle bekommen eine Rückmeldung.
                        self.status = if self.manual {
                            "Update-Prüfung fehlgeschlagen. Internetverbindung prüfen und später erneut versuchen."
                        } else {
                            ""
                        };
                    }
                }
            }
            UpdateEvent::Progress(value) => self.progress = Some(value.clamp(0, 100)),
            UpdateEvent::Downloaded(result) => {
                self.busy = false;
                self.deadline = None;
                self.progress = None;
                match result {
                    Ok(()) => {
                        self.policy.downloaded = true;
                        self.status = "Update heruntergeladen und geprüft.";
                        self.maybe_prepare();
                    }
                    Err(error) => {
                        log::warn!("Update fehlgeschlagen: {error}");
                        self.fail(
                            "Update konnte nicht installiert werden. Später erneut versuchen.",
                            true,
                        );
                    }
                }
            }
        }
        Action::None
    }

    /// Der Empfänger ist beendet: Velopack starten und App beenden lassen.
    pub fn apply_after_receiver_stopped(&mut self) -> Action {
        if !self.policy.preparing || self.policy.streaming {
            return Action::None;
        }
        let (Some(manager), Some(info)) = (self.manager.as_ref(), self.info.as_ref()) else {
            self.fail(
                "Update konnte nicht installiert werden. Später erneut versuchen.",
                true,
            );
            return Action::None;
        };
        match manager.wait_exit_then_apply_updates(
            &info.TargetFullRelease,
            false,
            true,
            Vec::<String>::new(),
        ) {
            Ok(()) => Action::Quit,
            Err(error) => {
                log::warn!("Update konnte nicht angewendet werden: {error}");
                self.fail(
                    "Update konnte nicht installiert werden. Später erneut versuchen.",
                    true,
                );
                Action::None
            }
        }
    }

    fn fail(&mut self, status: &'static str, show_banner: bool) {
        self.busy = false;
        self.progress = None;
        self.idle_grace = None;
        self.policy.reset();
        self.info = None;
        self.status = status;
        self.banner = show_banner;
    }

    pub fn visible(&self) -> bool {
        self.banner
    }
    pub fn can_install(&self) -> bool {
        !self.busy && !self.policy.preparing && self.info.is_some()
    }
    pub fn applying(&self) -> bool {
        self.policy.preparing
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    fn version(&self) -> String {
        self.info
            .as_ref()
            .map(|i| i.TargetFullRelease.Version.clone())
            .unwrap_or_default()
    }

    /// Text der Update-Leiste.
    pub fn message(&self) -> String {
        if self.policy.preparing {
            return tr("Update wird installiert …");
        }
        if let Some(progress) = self.progress {
            return trf(
                "Update wird heruntergeladen … %1 %",
                &[&progress.to_string()],
            );
        }
        if self.policy.downloaded && self.policy.requested && self.policy.streaming {
            return tr("Update bereit. Installation nach dem Spiegeln.");
        }
        if self.policy.downloaded {
            return trf("Update bereit (v%1)", &[&self.version()]);
        }
        self.status_text()
    }

    /// Statuszeile in den Einstellungen.
    pub fn status_text(&self) -> String {
        trf(self.status, &[&self.version()])
    }
}

// SPDX-License-Identifier: GPL-3.0-or-later
//! Steuerung der Oberfläche: verbindet Fenster, Empfänger, Updates und Diagnose.
//! Alles hier läuft im UI-Thread; Hintergrundarbeit meldet sich über `events`.
use crate::diagnostics;
use crate::events::{self, Event};
use crate::i18n::{self, tr, trf, Text};
use crate::logging;
use crate::receiver::{self, Notice, Receiver};
use crate::settings::{self, Geometry, Settings};
use crate::snapshot;
use crate::updater::{Action, UpdateService};
use crate::video_host::VideoHost;
use crate::win::{self, RawHwnd};
use crate::{DiagnosticsWindow, MainWindow, SettingsWindow, ToolbarWindow, T};
use slint::{
    CloseRequestResponse, Color, ComponentHandle, PhysicalPosition, PhysicalSize, Timer, TimerMode,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const VERSION: &str = env!("PCMIRROR_VERSION");
const TOOLBAR_HIDE_AFTER: Duration = Duration::from_millis(2600);
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(6);
const VK_ESCAPE: u32 = 0x1B;
const VK_F11: u32 = 0x7A;

fn color(hex: u32) -> Color {
    Color::from_rgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
const MUTED: u32 = 0xA0A0A0;
const WAITING: u32 = 0xC1A55B;
const CONNECTED: u32 = 0x6FA77B;
const FAILED: u32 = 0xC87878;

macro_rules! install_tr {
    ($window:expr) => {{
        let global = $window.global::<T>();
        global.set_lang(i18n::language().into());
        global.on_t(|source, _| tr(&source).into());
    }};
}

#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    Settings,
    Diagnostics,
}

/// Prüfergebnis für die Anzeige: 0 = unbekannt/prüft, 1 = in Ordnung, 2 = Handlungsbedarf.
fn check_state(value: Option<bool>) -> i32 {
    match value {
        Some(true) => 1,
        Some(false) => 2,
        None => 0,
    }
}

struct Snapshot {
    target: PathBuf,
    temporary: PathBuf,
    deadline: Instant,
}

struct State {
    settings: Settings,
    streaming: bool,
    restarting: bool,
    closing: bool,
    preparing_update: bool,
    firewall_banner: bool,
    status: Text,
    status_color: u32,
    error: Option<Text>,
    pin: Option<String>,
    notice: Option<(Text, Option<Instant>)>,
    firewall_status: Text,
    network_private: Option<bool>,
    firewall_rules: Option<bool>,
    firewall_checking: bool,
    last_activity: Instant,
    previous_cursor: (i32, i32),
    snapshot: Option<Snapshot>,
    converting: bool,
    relaunch_at: Option<Instant>,
    main_hwnd: Option<RawHwnd>,
    toolbar_hwnd: Option<RawHwnd>,
    toolbar_shown: bool,
    video_rect: (i32, i32, i32, i32, bool),
    original_language: String,
    log_revision: u64,
    /// Offene Dialoge, innerster zuletzt; das HWND kommt asynchron nach `show()`.
    modal: Vec<(Dialog, Option<RawHwnd>)>,
}

pub struct App {
    main: MainWindow,
    toolbar: ToolbarWindow,
    settings_window: RefCell<Option<SettingsWindow>>,
    diagnostics_window: RefCell<Option<DiagnosticsWindow>>,
    onboarding: Cell<bool>,
    state: RefCell<State>,
    receiver: RefCell<Receiver>,
    updater: RefCell<UpdateService>,
    video: RefCell<Option<VideoHost>>,
    events: mpsc::Receiver<Event>,
    timer: Timer,
    synthetic: bool,
    preview: bool,
    draining: Cell<bool>,
}

pub fn run(settings: Settings, synthetic: bool, preview: bool) -> Result<(), slint::PlatformError> {
    receiver::set_classic(settings.classic_receiver);
    let events = events::init();
    let main = MainWindow::new()?;
    let toolbar = ToolbarWindow::new()?;
    install_tr!(main);
    install_tr!(toolbar);

    let state = State {
        original_language: settings.language.clone(),
        settings,
        streaming: false,
        restarting: false,
        closing: false,
        preparing_update: false,
        firewall_banner: false,
        status: Text::new("Startet"),
        status_color: MUTED,
        error: None,
        pin: None,
        notice: synthetic.then(|| (Text::new("Synthetischer Bildtest"), None)),
        firewall_status: Text::new("Windows fragt zum Einrichten nach Administratorrechten."),
        network_private: None,
        firewall_rules: None,
        firewall_checking: false,
        last_activity: Instant::now(),
        previous_cursor: win::cursor_pos(),
        snapshot: None,
        converting: false,
        relaunch_at: None,
        main_hwnd: None,
        toolbar_hwnd: None,
        toolbar_shown: false,
        video_rect: (0, 0, 0, 0, false),
        log_revision: 0,
        modal: Vec::new(),
    };
    let app = Rc::new(App {
        main,
        toolbar,
        settings_window: RefCell::new(None),
        diagnostics_window: RefCell::new(None),
        onboarding: Cell::new(false),
        state: RefCell::new(state),
        receiver: RefCell::new(Receiver::default()),
        updater: RefCell::new(UpdateService::new()),
        video: RefCell::new(None),
        events,
        timer: Timer::default(),
        synthetic,
        preview,
        draining: Cell::new(false),
    });
    let weak = Rc::downgrade(&app);
    events::set_handler(move || {
        if let Some(app) = weak.upgrade() {
            app.drain();
        }
    });
    app.connect_main();
    app.connect_toolbar();
    if !preview {
        if let Some(g) = settings::load_geometry() {
            if win::on_any_monitor(g.x, g.y, g.width, g.height) {
                app.main
                    .window()
                    .set_position(PhysicalPosition::new(g.x, g.y));
                app.main
                    .window()
                    .set_size(PhysicalSize::new(g.width, g.height));
            }
        }
    }
    app.render();
    app.main.show()?;

    let weak = Rc::downgrade(&app);
    Timer::single_shot(Duration::from_millis(0), move || {
        if let Some(app) = weak.upgrade() {
            app.after_shown(0);
        }
    });
    let weak = Rc::downgrade(&app);
    app.timer
        .start(TimerMode::Repeated, Duration::from_millis(100), move || {
            if let Some(app) = weak.upgrade() {
                app.tick();
            }
        });
    slint::run_event_loop_until_quit()?;
    // Falls die Schleife anders endet: Empfänger nie zurücklassen.
    app.receiver.borrow_mut().shutdown();
    Ok(())
}

impl App {
    fn weak(self: &Rc<Self>) -> Weak<Self> {
        Rc::downgrade(self)
    }

    // ---------------------------------------------------------------- Aufbau

    fn connect_main(self: &Rc<Self>) {
        let main = &self.main;
        let w = self.weak();
        main.on_open_settings(move || {
            if let Some(app) = w.upgrade() {
                app.open_settings();
            }
        });
        let w = self.weak();
        main.on_show_diagnostics(move || {
            if let Some(app) = w.upgrade() {
                app.open_diagnostics();
            }
        });
        let w = self.weak();
        main.on_error_action(move || {
            if let Some(app) = w.upgrade() {
                if app.state.borrow().firewall_banner {
                    let public = app.public_network();
                    app.configure_firewall(public);
                } else {
                    app.restart();
                }
            }
        });
        let w = self.weak();
        main.on_onboarding_configure_firewall(move || {
            if let Some(app) = w.upgrade() {
                let public = app.public_network();
                app.configure_firewall(public);
            }
        });
        let w = self.weak();
        main.on_onboarding_recheck(move || {
            if let Some(app) = w.upgrade() {
                app.main
                    .set_onboarding_network(diagnostics::adapter_summary().into());
                app.check_firewall();
            }
        });
        let w = self.weak();
        main.on_onboarding_network_settings(move || {
            if let Some(app) = w.upgrade() {
                // Freigabe und Einstufung als Heimnetz in einem UAC-Schritt.
                app.configure_firewall(true);
            }
        });
        let w = self.weak();
        main.on_onboarding_finish(move || {
            if let Some(app) = w.upgrade() {
                app.close_onboarding();
            }
        });
        let w = self.weak();
        main.on_restart(move || {
            if let Some(app) = w.upgrade() {
                app.restart();
            }
        });
        let w = self.weak();
        main.on_dismiss_error(move || {
            if let Some(app) = w.upgrade() {
                app.state.borrow_mut().error = None;
                app.render();
            }
        });
        let w = self.weak();
        main.on_install_update(move || {
            if let Some(app) = w.upgrade() {
                app.updater.borrow_mut().install();
                app.render();
            }
        });
        let w = self.weak();
        main.on_later_update(move || {
            if let Some(app) = w.upgrade() {
                app.updater.borrow_mut().postpone();
                app.render();
            }
        });
        let w = self.weak();
        main.on_toggle_fullscreen(move || {
            if let Some(app) = w.upgrade() {
                app.toggle_fullscreen();
            }
        });
        let w = self.weak();
        main.on_leave_fullscreen(move || {
            if let Some(app) = w.upgrade() {
                app.main.window().set_fullscreen(false);
            }
        });
        let w = self.weak();
        main.window().on_close_requested(move || {
            if let Some(app) = w.upgrade() {
                app.quit(true);
            }
            CloseRequestResponse::HideWindow
        });
    }

    fn connect_toolbar(self: &Rc<Self>) {
        let toolbar = &self.toolbar;
        let w = self.weak();
        toolbar.on_fullscreen(move || {
            if let Some(app) = w.upgrade() {
                app.toggle_fullscreen();
            }
        });
        let w = self.weak();
        toolbar.on_screenshot(move || {
            if let Some(app) = w.upgrade() {
                app.screenshot();
            }
        });
        let w = self.weak();
        toolbar.on_toggle_mute(move || {
            if let Some(app) = w.upgrade() {
                app.toggle_mute();
            }
        });
        let w = self.weak();
        toolbar.on_disconnect(move || {
            if let Some(app) = w.upgrade() {
                app.restart();
            }
        });
        let w = self.weak();
        toolbar.on_pointer_activity(move || {
            if let Some(app) = w.upgrade() {
                app.state.borrow_mut().last_activity = Instant::now();
            }
        });
        toolbar
            .window()
            .on_close_requested(|| CloseRequestResponse::KeepWindowShown);
    }

    /// Erst wenn das Hauptfenster existiert, gibt es ein HWND für Video und Werkzeugleiste.
    fn after_shown(self: &Rc<Self>, attempt: u32) {
        let Some(main_hwnd) = win::hwnd_of(self.main.window()) else {
            // Das native Fenster entsteht erst, wenn die Ereignisschleife läuft.
            if attempt < 100 {
                let weak = self.weak();
                Timer::single_shot(Duration::from_millis(50), move || {
                    if let Some(app) = weak.upgrade() {
                        app.after_shown(attempt + 1);
                    }
                });
            } else {
                logging::line("FEHLER: Hauptfenster hat kein natives Handle");
            }
            return;
        };
        win::style_window(main_hwnd);
        let (topmost, automatic) = {
            let mut state = self.state.borrow_mut();
            state.main_hwnd = Some(main_hwnd);
            (state.settings.topmost, state.settings.automatic_updates)
        };
        if topmost {
            win::set_topmost(main_hwnd, true);
        }
        if !self.preview {
            if let Some(g) = settings::load_geometry() {
                if g.maximized {
                    self.main.window().set_maximized(true);
                }
            }
        }
        *self.video.borrow_mut() = VideoHost::create(main_hwnd);

        // Werkzeugleiste einmal außerhalb des Bildschirms erzeugen und dann nativ steuern,
        // damit sie beim Einblenden nie den Fokus stiehlt.
        self.toolbar
            .window()
            .set_position(PhysicalPosition::new(-32000, -32000));
        let _ = self.toolbar.show();
        self.attach_native_windows();

        if self.preview {
            self.set_status("Bereit", WAITING);
            return;
        }
        self.launch();
        if !self.synthetic {
            self.check_firewall();
            self.updater.borrow_mut().start(automatic);
            if !settings::onboarding_done() {
                self.open_onboarding(0);
            }
        }
    }

    fn check_firewall(&self) {
        self.state.borrow_mut().firewall_checking = true;
        diagnostics::check_firewall();
        self.render();
    }

    /// `make_private`: verbundene öffentliche Netzwerke zusätzlich als privat einstufen.
    fn configure_firewall(&self, make_private: bool) {
        self.state.borrow_mut().firewall_checking = true;
        logging::line(if make_private {
            "Firewall-Einrichtung gestartet (mit Netzwerk als privat)"
        } else {
            "Firewall-Einrichtung gestartet"
        });
        diagnostics::configure_firewall(make_private);
        self.render();
    }

    /// Öffentliches Netz aktiv und Empfänger dort nicht erreichbar.
    fn public_network(&self) -> bool {
        let state = self.state.borrow();
        state.network_private == Some(false) && state.firewall_rules != Some(true)
    }

    // ------------------------------------------------------------ Ereignisse

    fn drain(self: &Rc<Self>) {
        if self.draining.replace(true) {
            return; // verschachtelter Aufruf (z. B. während eines Dateidialogs)
        }
        while let Ok(event) = self.events.try_recv() {
            self.handle_event(event);
        }
        self.draining.set(false);
    }

    fn handle_event(self: &Rc<Self>, event: Event) {
        match event {
            Event::Receiver { generation, event } => {
                let notices = self.receiver.borrow_mut().handle(generation, event);
                self.handle_notices(notices);
            }
            Event::Update { generation, event } => {
                let action = self.updater.borrow_mut().handle(generation, event);
                self.handle_update_action(action);
            }
            Event::FirewallChecked {
                ok,
                private,
                rules,
                message,
                details,
            } => {
                {
                    let mut state = self.state.borrow_mut();
                    state.network_private = private;
                    state.firewall_rules = rules;
                    state.firewall_checking = false;
                }
                logging::line(&format!(
                    "{message}{}",
                    if details.is_empty() {
                        String::new()
                    } else {
                        format!("\n{details}")
                    }
                ));
                self.firewall_result(ok, message);
            }
            Event::FirewallConfigured { result } => match result {
                Ok(code) => {
                    logging::line(&match code {
                        0 => "Firewall-Einrichtung erfolgreich".to_owned(),
                        3 => "Firewall-Einrichtung: Empfänger nicht gefunden".to_owned(),
                        4 => "Firewall eingerichtet, Netzwerk konnte nicht als privat eingestuft werden".to_owned(),
                        _ => format!("Firewall-Einrichtung fehlgeschlagen, Code {code}"),
                    });
                    if code == 4 {
                        // Einstufung ging nicht automatisch: Windows-Seite zum Umstellen öffnen.
                        win::open_uri("ms-settings:network-status");
                    }
                    self.check_firewall();
                }
                Err(message) => {
                    logging::line(message);
                    self.state.borrow_mut().firewall_checking = false;
                    self.firewall_result(false, message);
                }
            },
            Event::SnapshotSaved { result } => {
                let target = {
                    let mut state = self.state.borrow_mut();
                    state.converting = false;
                    state.snapshot.take().map(|s| s.target)
                };
                match result {
                    Ok(path) => {
                        logging::line(&format!("Screenshot gespeichert: {}", path.display()));
                        self.state.borrow_mut().notice =
                            Some((Text::new("Screenshot gespeichert"), Some(Instant::now() + Duration::from_secs(3))));
                    }
                    Err(error) => self.show_error(
                        "Der Screenshot konnte nicht gespeichert werden. Bitte Bildempfang und Schreibrechte prüfen.",
                        &format!("{}: {error}", target.unwrap_or_default().display()),
                    ),
                }
                self.render();
            }
            Event::VideoDoubleClick => self.toggle_fullscreen(),
            Event::VideoKey { key } => match key {
                VK_F11 => self.toggle_fullscreen(),
                VK_ESCAPE => self.main.window().set_fullscreen(false),
                _ => {}
            },
            Event::VideoPointer => self.state.borrow_mut().last_activity = Instant::now(),
        }
    }

    fn handle_notices(self: &Rc<Self>, notices: Vec<Notice>) {
        for notice in notices {
            match notice {
                Notice::Ready => self.set_status("Bereit", WAITING),
                Notice::Frame(width, height) => {
                    let first = !self.state.borrow().streaming;
                    self.set_streaming(true);
                    self.state.borrow_mut().pin = None;
                    self.set_status("Verbunden", CONNECTED);
                    if first {
                        logging::line(&format!("Verbindung hergestellt: Bild {width} × {height}"));
                        self.connection_verified();
                    } else {
                        logging::line(&format!("Bildgröße geändert: {width} × {height}"));
                    }
                }
                Notice::Idle => {
                    if self.state.borrow().streaming {
                        logging::line("Verbindung beendet");
                    }
                    self.set_streaming(false);
                    self.set_status("Bereit für eine neue Verbindung", WAITING);
                }
                Notice::Pin(pin) => {
                    self.set_streaming(false);
                    self.state.borrow_mut().pin = Some(pin);
                    self.set_status("PIN am Gerät eingeben", WAITING);
                }
                Notice::Snapshot(ok) => self.finish_screenshot(ok),
                Notice::Log(line) => logging::line(&line),
                Notice::Error { message, details } => self.show_error(message, &details),
                Notice::Stopped => {
                    self.set_streaming(false);
                    let (preparing, restarting, closing) = {
                        let s = self.state.borrow();
                        (s.preparing_update, s.restarting, s.closing)
                    };
                    if preparing {
                        let action = self.updater.borrow_mut().apply_after_receiver_stopped();
                        self.handle_update_action(action);
                    } else if restarting && !closing {
                        let mut state = self.state.borrow_mut();
                        state.restarting = false;
                        state.relaunch_at = Some(Instant::now() + Duration::from_millis(300));
                    }
                }
            }
        }
        self.render();
    }

    fn handle_update_action(self: &Rc<Self>, action: Action) {
        match action {
            Action::None => {}
            Action::PrepareInstall => {
                if self.state.borrow().streaming {
                    return;
                }
                {
                    let mut state = self.state.borrow_mut();
                    state.preparing_update = true;
                    state.restarting = false;
                }
                let notices = self.receiver.borrow_mut().stop();
                self.handle_notices(notices);
            }
            Action::Quit => {
                logging::line("Update wird angewendet; App beendet sich.");
                self.quit(false);
                return;
            }
        }
        // Fehlgeschlagene Installation: Empfänger wieder bereitstellen.
        let resume = {
            let mut state = self.state.borrow_mut();
            let resume =
                state.preparing_update && !self.updater.borrow().applying() && !state.closing;
            if resume {
                state.preparing_update = false;
            }
            resume
        };
        if resume {
            self.launch();
        }
        self.render();
    }

    /// Eine echte Verbindung beweist, dass Firewall und Netzwerk passen: Warnung weg
    /// und gemerkt, damit eine ungenaue Prüfung später keinen Fehlalarm mehr zeigt.
    fn connection_verified(self: &Rc<Self>) {
        if self.synthetic || self.preview {
            return;
        }
        if !settings::connection_verified() {
            settings::set_connection_verified();
        }
        let mut state = self.state.borrow_mut();
        if state.firewall_banner {
            state.firewall_banner = false;
            state.error = None;
        }
    }

    fn firewall_result(self: &Rc<Self>, ok: bool, message: &'static str) {
        {
            let mut state = self.state.borrow_mut();
            state.firewall_status = Text::new(message);
            let onboarding = self.onboarding.get();
            let verified = state.streaming || settings::connection_verified();
            if !ok && !self.synthetic && !self.preview && !onboarding && !verified {
                state.firewall_banner = true;
                state.error = Some(Text::new(message));
            } else if ok && state.firewall_banner {
                state.firewall_banner = false;
                state.error = None;
            }
        }
        self.render();
    }

    // ---------------------------------------------------------------- Ablauf

    fn tick(self: &Rc<Self>) {
        let notices = self.receiver.borrow_mut().tick();
        if !notices.is_empty() {
            self.handle_notices(notices);
        }
        let action = self.updater.borrow_mut().tick();
        if action != Action::None {
            self.handle_update_action(action);
            self.render();
        }

        let now = Instant::now();
        let (relaunch, snapshot_expired, notice_expired) = {
            let state = self.state.borrow();
            (
                state.relaunch_at.is_some_and(|t| now > t),
                !state.converting && state.snapshot.as_ref().is_some_and(|s| now > s.deadline),
                state
                    .notice
                    .as_ref()
                    .is_some_and(|(_, until)| until.is_some_and(|t| now > t)),
            )
        };
        if relaunch {
            self.state.borrow_mut().relaunch_at = None;
            self.launch();
        }
        if snapshot_expired {
            self.finish_screenshot(false);
        }
        if notice_expired {
            let mut state = self.state.borrow_mut();
            state.notice = self
                .synthetic
                .then(|| (Text::new("Synthetischer Bildtest"), None));
            drop(state);
            self.render();
        }
        self.attach_native_windows();
        self.place_video();
        self.update_toolbar();
        self.refresh_diagnostics_log();
    }

    fn launch(self: &Rc<Self>) {
        if self.preview || self.state.borrow().preparing_update {
            return;
        }
        self.set_status("Startet", MUTED);
        let host = self.video.borrow().as_ref().map(VideoHost::raw);
        let Some(host) = host else {
            self.show_error("Der AirPlay-Empfänger konnte nicht gestartet werden. Bitte die App vollständig neu installieren.", "Video-Host fehlt");
            return;
        };
        let settings = self.state.borrow().settings.clone();
        let notices = self
            .receiver
            .borrow_mut()
            .start(&settings, host, self.synthetic);
        self.handle_notices(notices);
    }

    fn restart(self: &Rc<Self>) {
        {
            let mut state = self.state.borrow_mut();
            if state.restarting || state.preparing_update {
                return;
            }
            state.error = None;
            state.firewall_banner = false;
            state.pin = None;
            state.restarting = true;
        }
        self.set_status("Startet neu", MUTED);
        let notices = self.receiver.borrow_mut().stop();
        self.handle_notices(notices);
    }

    fn quit(self: &Rc<Self>, stop_receiver: bool) {
        {
            let mut state = self.state.borrow_mut();
            if state.closing && stop_receiver {
                return;
            }
            state.closing = true;
            state.restarting = false;
            if let Some(handle) = state.toolbar_hwnd {
                win::hide(handle);
            }
        }
        if !self.preview {
            self.save_geometry();
        }
        if let Some(window) = self.settings_window.borrow_mut().take() {
            let _ = window.hide();
        }
        if let Some(window) = self.diagnostics_window.borrow_mut().take() {
            let _ = window.hide();
        }
        if stop_receiver {
            self.receiver.borrow_mut().shutdown();
        }
        let _ = self.main.hide();
        let _ = self.toolbar.hide();
        let _ = slint::quit_event_loop();
    }

    fn save_geometry(&self) {
        let window = self.main.window();
        if window.is_fullscreen() {
            return;
        }
        let maximized = window.is_maximized();
        let position = window.position();
        let size = window.size();
        let mut geometry = settings::load_geometry().unwrap_or(Geometry {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            maximized,
        });
        geometry.maximized = maximized;
        if !maximized && !win::is_minimized(self.state.borrow().main_hwnd.unwrap_or_default()) {
            geometry = Geometry {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                maximized,
            };
        }
        settings::save_geometry(geometry);
    }

    fn set_status(&self, source: &str, color: u32) {
        let mut state = self.state.borrow_mut();
        state.status = Text::new(source);
        state.status_color = color;
        drop(state);
        self.render();
    }

    fn set_streaming(self: &Rc<Self>, streaming: bool) {
        {
            let mut state = self.state.borrow_mut();
            if state.streaming == streaming {
                return;
            }
            state.streaming = streaming;
            state.last_activity = Instant::now();
        }
        // Ohne Video kein Vollbild: Sonst bleibt das Fenster im Vollbildzustand mit der
        // Höhe der Wartefläche hängen und erscheint als schmaler Streifen über den Bildschirm.
        if !streaming && self.main.window().is_fullscreen() {
            self.main.window().set_fullscreen(false);
        }
        self.updater.borrow_mut().set_streaming(streaming);
        self.render();
    }

    fn show_error(self: &Rc<Self>, message: &str, details: &str) {
        logging::line(&format!("FEHLER: {message}\n{details}"));
        {
            let mut state = self.state.borrow_mut();
            state.firewall_banner = false;
            state.error = Some(Text::new(message));
        }
        self.set_status("Fehler", FAILED);
    }

    fn toggle_fullscreen(self: &Rc<Self>) {
        let window = self.main.window();
        window.set_fullscreen(!window.is_fullscreen());
        self.state.borrow_mut().last_activity = Instant::now();
    }

    fn toggle_mute(self: &Rc<Self>) {
        let muted = {
            let mut state = self.state.borrow_mut();
            state.settings.muted = !state.settings.muted;
            state.settings.muted
        };
        let notices = self.receiver.borrow_mut().mute(muted);
        self.handle_notices(notices);
        self.save_preferences();
        self.render();
    }

    fn save_preferences(self: &Rc<Self>) {
        let settings = self.state.borrow().settings.clone();
        if let Err(message) = settings.save() {
            self.show_error(message, "Einstellungen speichern");
        }
    }

    // ------------------------------------------------------------ Screenshot

    fn screenshot(self: &Rc<Self>) {
        {
            let state = self.state.borrow();
            if !state.streaming || state.snapshot.is_some() || state.converting {
                return;
            }
        }
        let pictures = std::env::var_os("USERPROFILE")
            .map(|p| PathBuf::from(p).join("Pictures"))
            .unwrap_or_default();
        let name = format!("PC-Mirror-{}.png", file_timestamp());
        // Kein Borrow halten: der Dialog hat eine eigene Nachrichtenschleife.
        let chosen = rfd::FileDialog::new()
            .set_title(tr("Screenshot speichern"))
            .add_filter(tr("PNG-Bild (*.png)"), &["png"])
            .set_directory(pictures)
            .set_file_name(name)
            .set_parent(&self.main.window().window_handle())
            .save_file();
        let Some(mut target) = chosen else { return };
        if !target
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
        {
            target.as_mut_os_string().push(".png");
        }
        if !self.state.borrow().streaming {
            return;
        }
        let temporary = std::env::temp_dir().join(format!(
            "pc-mirror-{}-{}.ppm",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        self.state.borrow_mut().snapshot = Some(Snapshot {
            target,
            temporary: temporary.clone(),
            deadline: Instant::now() + SNAPSHOT_TIMEOUT,
        });
        let notices = self.receiver.borrow_mut().snapshot(&temporary);
        self.handle_notices(notices);
        self.render();
    }

    fn finish_screenshot(self: &Rc<Self>, ok: bool) {
        let job = {
            let mut state = self.state.borrow_mut();
            if state.converting {
                return;
            }
            let Some(snapshot) = state.snapshot.as_ref() else {
                return;
            };
            let job = (snapshot.temporary.clone(), snapshot.target.clone());
            if ok {
                state.converting = true;
            } else {
                state.snapshot = None;
            }
            job
        };
        let (temporary, target) = job;
        if !ok {
            let _ = std::fs::remove_file(&temporary);
            self.show_error(
                "Der Screenshot konnte nicht gespeichert werden. Bitte Bildempfang und Schreibrechte prüfen.",
                &target.display().to_string(),
            );
            return;
        }
        // PNG-Kodierung großer Frames dauert; außerhalb des UI-Threads.
        std::thread::spawn(move || {
            let result = snapshot::ppm_to_png(&temporary, &target).map(|_| target);
            let _ = std::fs::remove_file(&temporary);
            events::post(Event::SnapshotSaved { result });
        });
        self.render();
    }

    // ------------------------------------------------------ Video & Leiste

    /// Hält das Host-Fenster deckungsgleich mit der Bühne der Oberfläche.
    fn place_video(&self) {
        let video = self.video.borrow();
        let Some(video) = video.as_ref() else { return };
        let scale = self.main.window().scale_factor();
        let rect = (
            (self.main.get_stage_x() * scale).round() as i32,
            (self.main.get_stage_y() * scale).round() as i32,
            (self.main.get_stage_width() * scale).round() as i32,
            (self.main.get_stage_height() * scale).round() as i32,
            self.state.borrow().streaming,
        );
        let mut state = self.state.borrow_mut();
        if state.video_rect != rect {
            state.video_rect = rect;
            video.place(rect.0, rect.1, rect.2, rect.3, rect.4);
        }
    }

    fn update_toolbar(&self) {
        let mut state = self.state.borrow_mut();
        let (Some(main), Some(toolbar)) = (state.main_hwnd, state.toolbar_hwnd) else {
            return;
        };
        let cursor = win::cursor_pos();
        let inside = win::window_rect(main).is_some_and(|r| {
            cursor.0 >= r.left && cursor.0 < r.right && cursor.1 >= r.top && cursor.1 < r.bottom
        });
        if cursor != state.previous_cursor && inside {
            state.last_activity = Instant::now();
        }
        state.previous_cursor = cursor;
        let over_toolbar = state.toolbar_shown
            && win::window_rect(toolbar).is_some_and(|r| {
                cursor.0 >= r.left && cursor.0 < r.right && cursor.1 >= r.top && cursor.1 < r.bottom
            });
        let modal_open = !state.modal.is_empty();
        let show = state.streaming
            && !state.closing
            && !modal_open
            && win::is_visible(main)
            && !win::is_minimized(main)
            && (state.last_activity.elapsed() < TOOLBAR_HIDE_AFTER || over_toolbar);
        if show {
            let scale = self.main.window().scale_factor();
            let (origin_x, origin_y) = win::client_origin(main);
            let client = self.main.window().size();
            let size = self.toolbar.window().size();
            let x = origin_x + client.width as i32 / 2 - size.width as i32 / 2;
            let y = origin_y + client.height as i32 - (24.0 * scale) as i32 - size.height as i32;
            win::show_at(toolbar, x, y);
            if !state.toolbar_shown {
                // Nativ eingeblendet: Der Software-Renderer zeichnet sonst nur geänderte
                // Bereiche neu, die Symbole fehlten bis zum Überfahren. Ein Wechsel der
                // Hintergrundphase macht die ganze Leiste ungültig.
                self.toolbar.set_repaint_phase(!self.toolbar.get_repaint_phase());
                self.toolbar.window().request_redraw();
            }
            state.toolbar_shown = true;
        } else if state.toolbar_shown {
            win::park(toolbar);
            state.toolbar_shown = false;
        }
    }

    // ------------------------------------------------------------ Darstellung

    /// Überträgt den Zustand in alle offenen Fenster (auch nach Sprachwechsel).
    fn render(&self) {
        let state = self.state.borrow();
        let updater = self.updater.borrow();
        let main = &self.main;
        main.set_streaming(state.streaming);
        main.set_device_line(
            trf(
                "Kontrollzentrum → Bildschirmsynchronisierung → %1",
                &[&state.settings.name],
            )
            .into(),
        );
        main.set_status_text(state.status.render().into());
        main.set_status_color(color(state.status_color));
        main.set_pin_text(
            state
                .pin
                .as_ref()
                .map(|p| trf("Code: %1", &[p]))
                .unwrap_or_default()
                .into(),
        );
        main.set_error_visible(state.error.is_some());
        main.set_error_action_text(
            tr(if state.firewall_banner
                && state.network_private == Some(false)
                && state.firewall_rules != Some(true)
            {
                "Netzwerk freigeben"
            } else if state.firewall_banner {
                "Firewall einrichten"
            } else {
                "Neu starten"
            })
            .into(),
        );
        main.set_error_text(
            state
                .error
                .as_ref()
                .map(Text::render)
                .unwrap_or_default()
                .into(),
        );
        main.set_update_visible(updater.visible());
        main.set_update_text(updater.message().into());
        main.set_can_install(updater.can_install());
        main.set_can_later(!updater.applying());
        main.set_notice_text(
            state
                .notice
                .as_ref()
                .map(|(t, _)| t.render())
                .unwrap_or_default()
                .into(),
        );

        self.toolbar.set_muted(state.settings.muted);
        self.toolbar
            .set_can_shoot(state.streaming && state.snapshot.is_none() && !state.converting);
        self.toolbar.set_can_disconnect(state.streaming);

        // Maßgeblich ist, ob der Empfänger im aktiven Profil erreichbar ist.
        let firewall_state = if state.firewall_checking {
            0
        } else {
            check_state(state.firewall_rules)
        };
        let firewall_text = if state.firewall_checking {
            tr("Wird geprüft …")
        } else {
            state.firewall_status.render()
        };
        if let Some(window) = self.settings_window.borrow().as_ref() {
            window.set_update_status(updater.status_text().into());
            window.set_can_check(!updater.is_busy());
            window.set_firewall_status(firewall_text.clone().into());
            window.set_firewall_state(firewall_state);
        }
        main.set_onboarding_visible(self.onboarding.get());
        {
            let window = main;
            window.set_onboarding_device(state.settings.name.clone().into());
            let network = if state.firewall_checking {
                0
            } else if state.firewall_rules == Some(true) {
                1
            } else {
                check_state(state.network_private)
            };
            window.set_onboarding_network_state(network);
            window.set_onboarding_network_text(
                tr(match network {
                    1 => "Netzwerk passt.",
                    2 => "Öffentliches Netzwerk: Als privates Heimnetz einstufen, wenn es dein Heimnetz ist.",
                    _ => "Wird geprüft …",
                })
                .into(),
            );
            let rules = if state.firewall_checking {
                0
            } else {
                check_state(state.firewall_rules)
            };
            window.set_onboarding_firewall_state(rules);
            window.set_onboarding_firewall_text(
                tr(match rules {
                    1 => "Freigabe eingerichtet. PC-Mirror ist im Heimnetz erreichbar.",
                    2 => "Freigabe fehlt noch.",
                    _ => "Wird geprüft …",
                })
                .into(),
            );
        }
        if let Some(window) = self.diagnostics_window.borrow().as_ref() {
            window.set_firewall_status(state.firewall_status.render().into());
        }
    }

    fn apply_language(&self, language: &str) {
        i18n::set_language(language);
        let code: slint::SharedString = i18n::language().into();
        self.main.global::<T>().set_lang(code.clone());
        self.toolbar.global::<T>().set_lang(code.clone());
        if let Some(window) = self.settings_window.borrow().as_ref() {
            window.global::<T>().set_lang(code.clone());
            window.set_version_text(trf("Version %1", &[VERSION]).into());
        }
        if let Some(window) = self.diagnostics_window.borrow().as_ref() {
            window.global::<T>().set_lang(code.clone());
            window.set_network(diagnostics::network_summary().into());
        }
        if self.onboarding.get() {
            self.main
                .set_onboarding_network(diagnostics::adapter_summary().into());
        }
        self.render();
    }

    // ------------------------------------------------------------ Dialoge

    /// Native Eigenschaften setzen, sobald die Fenster ein HWND haben
    /// (Werkzeugleiste ohne Fokus, Dialoge modal zum jeweiligen Besitzer).
    fn attach_native_windows(&self) {
        let Some(main) = self.state.borrow().main_hwnd else {
            return;
        };
        if self.state.borrow().toolbar_hwnd.is_none() {
            if let Some(handle) = win::hwnd_of(self.toolbar.window()) {
                win::make_tool_window(handle);
                win::set_owner(handle, main);
                win::style_window(handle);
                win::park(handle);
                self.state.borrow_mut().toolbar_hwnd = Some(handle);
            }
        }
        let count = self.state.borrow().modal.len();
        for index in 0..count {
            let (dialog, attached) = self.state.borrow().modal[index];
            if attached.is_some() {
                continue;
            }
            let handle = match dialog {
                Dialog::Settings => self
                    .settings_window
                    .borrow()
                    .as_ref()
                    .and_then(|w| win::hwnd_of(w.window())),
                Dialog::Diagnostics => self
                    .diagnostics_window
                    .borrow()
                    .as_ref()
                    .and_then(|w| win::hwnd_of(w.window())),
            };
            let Some(handle) = handle else { break };
            let owner = if index == 0 {
                Some(main)
            } else {
                self.state.borrow().modal[index - 1].1
            };
            let Some(owner) = owner else { break };
            win::style_window(handle);
            win::set_owner(handle, owner);
            win::enable_window(owner, false);
            win::bring_to_front(handle);
            self.state.borrow_mut().modal[index].1 = Some(handle);
        }
    }

    fn begin_modal(&self, dialog: Dialog) {
        self.state.borrow_mut().modal.push((dialog, None));
        self.attach_native_windows();
    }

    fn end_modal(&self, dialog: Dialog) {
        let owner = {
            let mut state = self.state.borrow_mut();
            let Some(index) = state.modal.iter().position(|(d, _)| *d == dialog) else {
                return;
            };
            let (_, handle) = state.modal.remove(index);
            handle.and(if index == 0 {
                state.main_hwnd
            } else {
                state.modal[index - 1].1
            })
        };
        if let Some(owner) = owner {
            win::enable_window(owner, true);
            win::bring_to_front(owner);
        }
    }

    fn open_settings(self: &Rc<Self>) {
        if let Some(window) = self.settings_window.borrow().as_ref() {
            if let Some(handle) = win::hwnd_of(window.window()) {
                win::bring_to_front(handle);
            }
            return;
        }
        let Ok(window) = SettingsWindow::new() else {
            return;
        };
        install_tr!(window);
        {
            let mut state = self.state.borrow_mut();
            state.original_language = i18n::language().to_owned();
            let s = &state.settings;
            window.set_language_index(if i18n::language() == "en" { 1 } else { 0 });
            window.set_device_name(s.name.clone().into());
            window.set_quality_index(i32::from(s.quality));
            window.set_ipad_fill(s.ipad_fill);
            window.set_topmost(s.topmost);
            window.set_autostart(s.autostart);
            window.set_pin_enabled(s.pin_enabled);
            window.set_pin(s.pin.clone().into());
            window.set_software_decoder(s.software_decoder);
            window.set_classic_receiver(s.classic_receiver);
            window.set_automatic_updates(s.automatic_updates);
            window.set_version_text(trf("Version %1", &[VERSION]).into());
        }

        let w = self.weak();
        window.on_language_selected(move |index| {
            if let Some(app) = w.upgrade() {
                app.apply_language(if index == 1 { "en" } else { "de" });
            }
        });
        let w = self.weak();
        window.on_check_updates(move || {
            if let Some(app) = w.upgrade() {
                app.updater.borrow_mut().check(true);
                app.render();
            }
        });
        let w = self.weak();
        window.on_save(move || {
            if let Some(app) = w.upgrade() {
                app.save_settings();
            }
        });
        let w = self.weak();
        window.on_cancel(move || {
            if let Some(app) = w.upgrade() {
                app.close_settings(false);
            }
        });
        let w = self.weak();
        window.on_show_diagnostics(move || {
            if let Some(app) = w.upgrade() {
                app.open_diagnostics();
            }
        });
        let w = self.weak();
        window.on_configure_firewall(move || {
            if let Some(app) = w.upgrade() {
                let public = app.public_network();
                app.configure_firewall(public);
            }
        });
        let w = self.weak();
        window.on_start_onboarding(move || {
            if let Some(app) = w.upgrade() {
                app.close_settings(false);
                app.open_onboarding(1);
            }
        });
        let w = self.weak();
        window.window().on_close_requested(move || {
            if let Some(app) = w.upgrade() {
                app.close_settings(false);
            }
            CloseRequestResponse::HideWindow
        });
        if window.show().is_err() {
            return;
        }
        *self.settings_window.borrow_mut() = Some(window);
        self.begin_modal(Dialog::Settings);
        self.render();
    }

    fn save_settings(self: &Rc<Self>) {
        let next = {
            let borrow = self.settings_window.borrow();
            let Some(window) = borrow.as_ref() else {
                return;
            };
            let current = self.state.borrow().settings.clone();
            let next = Settings {
                name: window.get_device_name().trim().to_owned(),
                language: if window.get_language_index() == 1 {
                    "en".into()
                } else {
                    "de".into()
                },
                quality: window.get_quality_index().clamp(0, 2) as u8,
                autostart: window.get_autostart(),
                pin_enabled: window.get_pin_enabled(),
                pin: window.get_pin().trim().to_owned(),
                software_decoder: window.get_software_decoder(),
                classic_receiver: window.get_classic_receiver(),
                topmost: window.get_topmost(),
                muted: current.muted,
                automatic_updates: window.get_automatic_updates(),
                ipad_fill: window.get_ipad_fill(),
            };
            let validation = settings::validate_name(&next.name).and_then(|_| {
                if next.pin_enabled {
                    settings::validate_pin(&next.pin)
                } else {
                    Ok(())
                }
            });
            if let Err(message) = validation {
                window.set_validation(tr(message).into());
                return;
            }
            next
        };

        let previous = self.state.borrow().settings.clone();
        let saved = next.save();
        if let Err(message) = saved {
            if message != "Der Windows-Autostart konnte nicht geändert werden." {
                self.close_settings(false);
                self.show_error(message, "Einstellungen speichern");
                return;
            }
            self.show_error(message, "Einstellungen speichern");
        }
        let receiver_changed = next.receiver_differs(&previous);
        receiver::set_classic(next.classic_receiver);
        let (topmost, ipad_fill, automatic) =
            (next.topmost, next.ipad_fill, next.automatic_updates);
        {
            let mut state = self.state.borrow_mut();
            state.settings = next;
            state.settings.autostart = Settings::load().autostart;
            state.original_language = state.settings.language.clone();
        }
        self.close_settings(true);
        self.updater.borrow_mut().set_automatic(automatic);
        if let Some(main) = self.state.borrow().main_hwnd {
            win::set_topmost(main, topmost);
        }
        if !self.preview {
            if ipad_fill != previous.ipad_fill {
                let notices = self.receiver.borrow_mut().set_ipad_fill(ipad_fill);
                self.handle_notices(notices);
            }
            if receiver_changed {
                self.restart();
            }
        }
        self.render();
    }

    fn close_settings(self: &Rc<Self>, saved: bool) {
        let Some(window) = self.settings_window.borrow_mut().take() else {
            return;
        };
        if let Some(diagnostics) = self.diagnostics_window.borrow().as_ref() {
            // Diagnose wurde über die Einstellungen geöffnet: zuerst schließen.
            self.end_modal(Dialog::Diagnostics);
            let _ = diagnostics.hide();
        }
        self.diagnostics_window.borrow_mut().take();
        self.end_modal(Dialog::Settings);
        let _ = window.hide();
        if !saved {
            let original = self.state.borrow().original_language.clone();
            self.apply_language(&original);
        } else {
            let language = self.state.borrow().settings.language.clone();
            self.apply_language(&language);
        }
    }

    fn open_diagnostics(self: &Rc<Self>) {
        if let Some(window) = self.diagnostics_window.borrow().as_ref() {
            if let Some(handle) = win::hwnd_of(window.window()) {
                win::bring_to_front(handle);
            }
            return;
        }
        let Ok(window) = DiagnosticsWindow::new() else {
            return;
        };
        install_tr!(window);
        window.set_network(diagnostics::network_summary().into());
        window.set_log_text(logging::recent_newest_first().into());
        self.state.borrow_mut().log_revision = logging::revision();

        let w = self.weak();
        window.on_configure_firewall(move || {
            if let Some(app) = w.upgrade() {
                let public = app.public_network();
                app.configure_firewall(public);
            }
        });
        let w = self.weak();
        window.on_recheck(move || {
            if let Some(app) = w.upgrade() {
                app.check_firewall();
            }
        });
        let w = self.weak();
        window.on_open_logs(move || {
            if let Some(app) = w.upgrade() {
                let folder = logging::path()
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_default();
                if !win::open_folder(&folder) {
                    app.show_error(
                        "Der Logordner konnte nicht geöffnet werden.",
                        &folder.display().to_string(),
                    );
                }
            }
        });
        let w = self.weak();
        window.on_close_clicked(move || {
            if let Some(app) = w.upgrade() {
                app.close_diagnostics();
            }
        });
        let w = self.weak();
        window.window().on_close_requested(move || {
            if let Some(app) = w.upgrade() {
                app.close_diagnostics();
            }
            CloseRequestResponse::HideWindow
        });
        if window.show().is_err() {
            return;
        }
        *self.diagnostics_window.borrow_mut() = Some(window);
        self.begin_modal(Dialog::Diagnostics);
        self.state.borrow_mut().last_activity = Instant::now();
        diagnostics::check_firewall();
        self.render();
    }

    /// Zeigt den Assistenten im Hauptfenster (statt des Wartebildschirms).
    fn open_onboarding(self: &Rc<Self>, page: i32) {
        self.onboarding.set(true);
        self.main.set_onboarding_page(page);
        self.main
            .set_onboarding_network(diagnostics::adapter_summary().into());
        self.check_firewall();
    }

    /// Abschließen oder Überspringen merkt sich den Assistenten als erledigt.
    fn close_onboarding(self: &Rc<Self>) {
        if !self.onboarding.replace(false) {
            return;
        }
        settings::set_onboarding_done();
        // Offene Probleme nach dem Assistenten wieder als Hinweis zeigen.
        let (rules, private) = {
            let state = self.state.borrow();
            (state.firewall_rules, state.network_private)
        };
        if rules.is_some() {
            let (ok, message) = diagnostics::firewall_message(private, rules);
            self.firewall_result(ok, message);
        }
        self.render();
    }

    fn close_diagnostics(&self) {
        let Some(window) = self.diagnostics_window.borrow_mut().take() else {
            return;
        };
        self.end_modal(Dialog::Diagnostics);
        let _ = window.hide();
    }

    fn refresh_diagnostics_log(&self) {
        let revision = logging::revision();
        if self.state.borrow().log_revision == revision {
            return;
        }
        if let Some(window) = self.diagnostics_window.borrow().as_ref() {
            window.set_log_text(logging::recent_newest_first().into());
            self.state.borrow_mut().log_revision = revision;
        }
    }
}

fn file_timestamp() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

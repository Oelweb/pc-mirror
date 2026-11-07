// SPDX-License-Identifier: GPL-3.0-or-later
//! Hintergrund-Threads melden Ergebnisse über diesen Kanal; die Verarbeitung
//! läuft immer im UI-Thread (über `slint::invoke_from_event_loop`).
use crate::receiver::ReceiverEvent;
use crate::updater::UpdateEvent;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{mpsc, OnceLock};

pub enum Event {
    Receiver {
        generation: u64,
        event: ReceiverEvent,
    },
    Update {
        generation: u64,
        event: UpdateEvent,
    },
    FirewallChecked {
        ok: bool,
        private: Option<bool>,
        rules: Option<bool>,
        message: &'static str,
        details: String,
    },
    FirewallConfigured {
        result: Result<u32, &'static str>,
    },
    SnapshotSaved {
        result: Result<PathBuf, String>,
    },
    VideoDoubleClick,
    VideoKey {
        key: u32,
    },
    VideoPointer,
}

static SENDER: OnceLock<mpsc::Sender<Event>> = OnceLock::new();

thread_local! {
    static HANDLER: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

pub fn init() -> mpsc::Receiver<Event> {
    let (sender, receiver) = mpsc::channel();
    let _ = SENDER.set(sender);
    receiver
}

/// Im UI-Thread aufrufen: diese Funktion leert später den Kanal.
pub fn set_handler(handler: impl Fn() + 'static) {
    HANDLER.with(|slot| *slot.borrow_mut() = Some(Box::new(handler)));
}

/// Aus beliebigem Thread aufrufbar.
pub fn post(event: Event) {
    let Some(sender) = SENDER.get() else { return };
    if sender.send(event).is_ok() {
        let _ = slint::invoke_from_event_loop(|| {
            HANDLER.with(|slot| {
                if let Some(handler) = slot.borrow().as_ref() {
                    handler();
                }
            })
        });
    }
}

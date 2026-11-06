// SPDX-License-Identifier: GPL-3.0-or-later
//! Wann darf ein Update installiert werden? Wird vom echten Dienst und den Tests geteilt.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UpdatePolicy {
    pub streaming: bool,
    pub downloaded: bool,
    pub requested: bool,
    pub preparing: bool,
}

impl UpdatePolicy {
    /// Nur mit Zustimmung, nach fertigem Download und nie während einer Spiegelung.
    pub fn can_prepare(&self) -> bool {
        self.downloaded && self.requested && !self.streaming && !self.preparing
    }

    /// „Später“ widerruft auch einen vorgemerkten Neustart.
    pub fn postpone(&mut self) {
        self.requested = false;
    }

    /// Zurücksetzen nach Fehler/Zeitlimit; der Spiegelungszustand bleibt erhalten.
    pub fn reset(&mut self) {
        *self = UpdatePolicy {
            streaming: self.streaming,
            ..Default::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_streaming_interlock_later_and_duplicate_apply() {
        let mut p = UpdatePolicy::default();
        assert!(!p.can_prepare(), "Keine ungefragte Installation");
        p.downloaded = true;
        assert!(
            !p.can_prepare(),
            "Ein vorhandener Download ist keine Zustimmung"
        );
        p.requested = true;
        p.streaming = true;
        assert!(!p.can_prepare(), "Nie eine aktive Spiegelung neu starten");
        p.streaming = false;
        assert!(
            p.can_prepare(),
            "Nach Sitzungsende mit Zustimmung installieren"
        );
        p.preparing = true;
        assert!(!p.can_prepare(), "Nur ein Installationsvorgang");
        p.preparing = false;
        p.postpone();
        assert!(!p.can_prepare(), "Später widerruft den Neustart");
        p.requested = true;
        p.streaming = true;
        assert!(!p.can_prepare(), "Neue Frames brechen die Ruhefrist ab");
        p.downloaded = false;
        p.streaming = false;
        assert!(!p.can_prepare(), "Download muss fertig sein");
        p.streaming = true;
        p.reset();
        assert!(p.streaming && !p.downloaded && !p.requested);
    }
}

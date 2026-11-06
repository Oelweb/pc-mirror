// SPDX-License-Identifier: GPL-3.0-or-later
//! Deutsche Quelltexte, englische Übersetzung aus `translations/en.json`.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

static ENGLISH: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../translations/en.json")).unwrap_or_default()
});
static USE_ENGLISH: AtomicBool = AtomicBool::new(false);

/// Normalisiert unbekannte Werte auf Deutsch.
pub fn normalize(language: &str) -> &'static str {
    if language == "en" {
        "en"
    } else {
        "de"
    }
}

pub fn set_language(language: &str) {
    USE_ENGLISH.store(normalize(language) == "en", Ordering::Relaxed);
}

pub fn language() -> &'static str {
    if USE_ENGLISH.load(Ordering::Relaxed) {
        "en"
    } else {
        "de"
    }
}

pub fn tr(source: &str) -> String {
    if USE_ENGLISH.load(Ordering::Relaxed) {
        if let Some(text) = ENGLISH.get(source) {
            return text.clone();
        }
    }
    source.to_owned()
}

/// Übersetzt und ersetzt `%1`, `%2`, … der Reihe nach.
pub fn trf(source: &str, args: &[&str]) -> String {
    let mut text = tr(source);
    for (index, value) in args.iter().enumerate() {
        text = text.replace(&format!("%{}", index + 1), value);
    }
    text
}

/// Ein übersetzbarer Text mit Argumenten, der bei Sprachwechsel neu erzeugt wird.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub source: String,
    pub args: Vec<String>,
}

impl Text {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            args: Vec::new(),
        }
    }
    pub fn render(&self) -> String {
        if self.source.is_empty() {
            return String::new();
        }
        let args: Vec<&str> = self.args.iter().map(String::as_str).collect();
        trf(&self.source, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_formats() {
        set_language("en");
        assert_eq!(tr("Einstellungen"), "Settings");
        assert_eq!(
            trf("Update verfügbar (v%1)", &["1.2.3"]),
            "Update available (v1.2.3)"
        );
        assert_eq!(tr("unbekannt"), "unbekannt");
        set_language("xx");
        assert_eq!(language(), "de");
        assert_eq!(tr("Einstellungen"), "Einstellungen");
    }

    #[test]
    fn every_ui_string_has_a_translation() {
        let ui = include_str!("../ui/app.slint");
        let mut missing = Vec::new();
        for part in ui.split("T.t(\"").skip(1) {
            let source = part
                .split("\", T.lang")
                .next()
                .unwrap()
                .replace("\\n", "\n");
            if !ENGLISH.contains_key(&source) {
                missing.push(source);
            }
        }
        assert!(missing.is_empty(), "Fehlende Übersetzungen: {missing:#?}");
    }
}

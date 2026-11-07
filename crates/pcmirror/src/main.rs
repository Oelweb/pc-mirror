// SPDX-License-Identifier: GPL-3.0-or-later
//! PC-Mirror: native Windows-App für iPhone/iPad-Bildschirmsynchronisierung mit Ton.
#![windows_subsystem = "windows"]

mod app;
mod diagnostics;
mod events;
mod i18n;
mod logging;
mod network;
mod receiver;
mod secret;
mod settings;
mod signed_source;
mod snapshot;
mod update_policy;
mod updater;
mod video_host;
mod win;

slint::include_modules!();

fn main() {
    // Installations-/Update-Hooks von Velopack müssen vor allem anderen laufen.
    // Ein bereits geladenes Update wird nie ohne Zustimmung in der Oberfläche angewendet.
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();

    let arguments: Vec<String> = std::env::args().collect();
    // Erhöhter Hilfsprozess für die Firewall-Regeln: nur Regeln schreiben, keine Oberfläche.
    if arguments.iter().any(|a| a == diagnostics::CONFIGURE_ARG) {
        std::process::exit(diagnostics::apply_rules());
    }
    let synthetic = arguments.iter().any(|a| a == "--test-pattern");
    let preview = arguments.iter().any(|a| a == "--ui-preview");

    let settings = settings::Settings::load();
    i18n::set_language(&settings.language);

    if let Err(error) = logging::init() {
        let folder = logging::path()
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        win::message_box(
            &i18n::trf(
                "Der Logordner ist nicht beschreibbar:\n%1\n%2",
                &[&folder, &error],
            ),
            true,
        );
        std::process::exit(1);
    }

    let instance = if preview {
        "preview"
    } else if synthetic {
        "test"
    } else {
        "instance"
    };
    let Some(_lock) = win::SingleInstance::acquire(&format!("Local\\PC-Mirror-{instance}")) else {
        win::message_box(
            &i18n::tr(
                "PC-Mirror läuft bereits. Bitte das vorhandene Fenster über die Taskleiste öffnen.",
            ),
            false,
        );
        return;
    };

    logging::line(&format!("PC-Mirror {} startet", env!("PCMIRROR_VERSION")));
    if let Err(error) = select_renderer().and_then(|_| app::run(settings, synthetic, preview)) {
        logging::line(&format!("FEHLER: Oberfläche konnte nicht starten: {error}"));
        win::message_box(&format!("PC-Mirror: {error}"), true);
        std::process::exit(1);
    }
}

/// Software-Renderer: robust neben dem nativen D3D11-Videofenster und
/// unabhängig von OpenGL-Treibern.
fn select_renderer() -> Result<(), slint::PlatformError> {
    match slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()
    {
        Ok(()) => Ok(()),
        Err(error) => {
            logging::line(&format!(
                "Software-Renderer nicht verfügbar, Standard wird verwendet: {error}"
            ));
            Ok(())
        }
    }
}

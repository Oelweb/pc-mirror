// SPDX-License-Identifier: GPL-3.0-or-later
use std::{env, fs, path::PathBuf};

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = crate_dir.join("../..");

    // version.txt bleibt die einzige Versionsquelle für App, Installer und Feed.
    let version_file = root.join("version.txt");
    let version = fs::read_to_string(&version_file)
        .expect("version.txt fehlt")
        .trim()
        .to_owned();
    assert!(
        version.split('.').count() == 3 && version.split('.').all(|p| p.parse::<u32>().is_ok()),
        "version.txt muss eine stabile SemVer-Version enthalten"
    );
    println!("cargo:rerun-if-changed={}", version_file.display());
    println!("cargo:rustc-env=PCMIRROR_VERSION={version}");

    // Öffentlicher Schlüssel für signierte Update-Feeds.
    let key_file = root.join("packaging/update-signing.pub");
    let key = fs::read_to_string(&key_file)
        .expect("packaging/update-signing.pub fehlt")
        .trim()
        .to_owned();
    println!("cargo:rerun-if-changed={}", key_file.display());
    println!("cargo:rustc-env=PCMIRROR_UPDATE_KEY={key}");

    // Übersetzungsdatei auf gültiges JSON prüfen, damit Fehler beim Build auffallen.
    let translations = crate_dir.join("translations/en.json");
    println!("cargo:rerun-if-changed={}", translations.display());
    let text = fs::read_to_string(&translations).unwrap();
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text)
        .expect("en.json ist ungültig");

    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Slint-Oberfläche konnte nicht übersetzt werden");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let numeric: Vec<u64> = version.split('.').map(|p| p.parse().unwrap()).collect();
        let packed = (numeric[0] << 48) | (numeric[1] << 32) | (numeric[2] << 16);
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon(root.join("assets/app.ico").to_str().unwrap())
            .set_manifest(MANIFEST)
            .set("ProductName", "PC-Mirror")
            .set("FileDescription", "PC-Mirror")
            .set("CompanyName", "Oelweb")
            .set("LegalCopyright", "GPL-3.0-or-later")
            .set("ProductVersion", &version)
            .set("FileVersion", &version)
            .set_version_info(winresource::VersionInfo::PRODUCTVERSION, packed)
            .set_version_info(winresource::VersionInfo::FILEVERSION, packed);
        resource
            .compile()
            .expect("Windows-Ressourcen konnten nicht erstellt werden");
    }
}

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="Oelweb.PCMirror" version="1.0.0.0"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#;

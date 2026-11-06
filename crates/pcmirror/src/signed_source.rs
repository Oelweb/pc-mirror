// SPDX-License-Identifier: GPL-3.0-or-later
//! Update-Quelle, die nur einem Ed25519-signierten Feed vertraut.
//!
//! Velopack prüft jedes Paket gegen den SHA-256 aus dem Feed. Diese Quelle
//! stellt zusätzlich sicher, dass der Feed selbst vom Herausgeber stammt:
//! Ein kompromittierter Webserver kann so keine eigenen Pakete ausliefern.
use std::path::Path;
use std::sync::mpsc::Sender;
use velopack::bundle::Manifest;
use velopack::download::download_url_as_string;
use velopack::sources::{HttpSource, UpdateSource};
use velopack::{Error, HttpOptions, VelopackAsset, VelopackAssetFeed};

pub const PUBLIC_KEY: &str = env!("PCMIRROR_UPDATE_KEY");

pub struct SignedHttpSource {
    base: String,
    downloads: HttpSource,
}

impl SignedHttpSource {
    pub fn new(base: &str) -> Self {
        let options = HttpOptions {
            TimeoutMilliseconds: 15 * 60 * 1000,
            ..Default::default()
        };
        Self {
            base: base.trim_end_matches('/').to_owned(),
            downloads: HttpSource::new_with_options(base, options),
        }
    }
}

impl UpdateSource for SignedHttpSource {
    fn get_release_feed(
        &self,
        channel: &str,
        app: &Manifest,
        staged_user_id: &str,
    ) -> Result<VelopackAssetFeed, Error> {
        if !channel
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Error::Other("Ungültiger Update-Kanal".into()));
        }
        let options = HttpOptions {
            TimeoutMilliseconds: 30_000,
            ..Default::default()
        };
        let name = format!("releases.{channel}.json");
        let feed_url = format!(
            "{}/{name}?localVersion={}&id={}&stagingId={}",
            self.base,
            app.version,
            encode(&app.id),
            encode(staged_user_id)
        );
        let feed = download_url_as_string(&feed_url, Some(&options))?;
        let signature =
            download_url_as_string(&format!("{}/{name}.sig", self.base), Some(&options))
                .map_err(|e| Error::Other(format!("Signatur des Update-Feeds fehlt: {e}")))?;
        let parsed = verify_feed(feed.as_bytes(), &signature, PUBLIC_KEY)?;
        log::info!(
            "Update-Feed signiert und geprüft ({} Einträge)",
            parsed.Assets.len()
        );
        Ok(parsed)
    }

    fn download_release_entry(
        &self,
        asset: &VelopackAsset,
        local_file: &Path,
        progress: Option<Sender<i16>>,
    ) -> Result<(), Error> {
        // Name stammt aus dem signierten Feed; trotzdem keine Pfadbestandteile zulassen.
        if !is_plain_file_name(&asset.FileName) {
            return Err(Error::Other(format!(
                "Ungültiger Paketname: {}",
                asset.FileName
            )));
        }
        self.downloads
            .download_release_entry(asset, local_file, progress)
    }
}

/// Prüft Signatur und verlangt für jedes Paket einen SHA-256.
pub fn verify_feed(
    feed: &[u8],
    signature: &str,
    public_key: &str,
) -> Result<VelopackAssetFeed, Error> {
    pcmirror_sign::verify(feed, signature, public_key).map_err(Error::Other)?;
    let parsed: VelopackAssetFeed = serde_json::from_slice(feed)?;
    for asset in &parsed.Assets {
        let valid = asset.SHA256.len() == 64 && asset.SHA256.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid || !is_plain_file_name(&asset.FileName) {
            return Err(Error::Other(format!(
                "Feed-Eintrag ohne gültigen SHA-256 oder Namen: {}",
                asset.FileName
            )));
        }
    }
    Ok(parsed)
}

fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"{"Assets":[{"PackageId":"PCMirror","Version":"0.3.0","Type":"Full","FileName":"PCMirror-0.3.0-full.nupkg","SHA1":"a","SHA256":"0000000000000000000000000000000000000000000000000000000000000000","Size":1}]}"#;

    #[test]
    fn accepts_only_signed_feeds_with_sha256() {
        let (private, public) = pcmirror_sign::generate_keypair();
        let signature = pcmirror_sign::sign(FEED.as_bytes(), &private).unwrap();
        assert_eq!(
            verify_feed(FEED.as_bytes(), &signature, &public)
                .unwrap()
                .Assets
                .len(),
            1
        );

        let tampered = FEED.replace("0.3.0-full", "0.3.1-full");
        assert!(verify_feed(tampered.as_bytes(), &signature, &public).is_err());

        let unhashed = FEED.replace(
            "0000000000000000000000000000000000000000000000000000000000000000",
            "",
        );
        let signature = pcmirror_sign::sign(unhashed.as_bytes(), &private).unwrap();
        assert!(verify_feed(unhashed.as_bytes(), &signature, &public).is_err());

        let traversal = FEED.replace("PCMirror-0.3.0-full.nupkg", "..\\\\evil.exe");
        let signature = pcmirror_sign::sign(traversal.as_bytes(), &private).unwrap();
        assert!(verify_feed(traversal.as_bytes(), &signature, &public).is_err());
    }

    #[test]
    fn embedded_key_is_valid() {
        let (private, _) = pcmirror_sign::generate_keypair();
        let signature = pcmirror_sign::sign(b"x", &private).unwrap();
        // Falscher Schlüssel → Ablehnung, aber kein Formatfehler des eingebetteten Schlüssels.
        let error = pcmirror_sign::verify(b"x", &signature, PUBLIC_KEY).unwrap_err();
        assert!(error.contains("Signatur"), "{error}");
    }
}

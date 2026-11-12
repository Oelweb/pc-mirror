// SPDX-License-Identifier: GPL-3.0-or-later
//! Video- und Tonausgabe über GStreamer (Pipelines wie in UxPlays Renderern).
use crate::control;
use crate::gst::{self, Pipeline, GST_STATE_PLAYING};
use std::sync::Mutex;

pub struct Media {
    video_sink: String,
    audio_sink: String,
    software_decoder: bool,
    inner: Mutex<Inner>,
}

/// Zusätzlicher Puffer gegen Schwankungen im WLAN, bevor ein Bild als verspätet gilt.
const JITTER_SLACK_NS: u64 = 40_000_000;
/// Ab dieser Verspätung wird der Zeitbezug neu gesetzt.
const RESYNC_NS: u64 = 250_000_000;

/// Wandelt die Zeitstempel des Senders in Darstellungszeitpunkte der Pipeline um,
/// damit die Bilder im Takt des iPhones statt in Netzwerkschüben erscheinen
/// (wie `video_sync` in der C-Fassung).
#[derive(Default)]
pub struct VideoClock {
    /// Laufzeit minus Senderzeit (kleinste bisher gemessene Übertragungszeit).
    offset: Option<i128>,
    last_pts: Option<u64>,
}

impl VideoClock {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `remote_ns`: Senderzeit des Bildes, `now`: aktuelle Pipeline-Laufzeit.
    pub fn pts(&mut self, remote_ns: u64, now: u64) -> u64 {
        let arrival = i128::from(now) - i128::from(remote_ns);
        let offset = match self.offset {
            // Früher als erwartet: Bezug auf die schnellere Übertragung nachziehen.
            Some(offset) if arrival >= offset && arrival - offset <= i128::from(RESYNC_NS) => offset,
            Some(offset) if arrival < offset => arrival,
            _ => arrival, // erstes Bild oder stark verspätet: neu ausrichten
        };
        self.offset = Some(offset);
        let mut pts = (i128::from(remote_ns) + offset + i128::from(JITTER_SLACK_NS)).max(0) as u64;
        if let Some(last) = self.last_pts {
            pts = pts.max(last + 1); // Zeitstempel bleiben aufsteigend
        }
        self.last_pts = Some(pts);
        pts
    }
}

/// NTP-Zeitstempel (32.32) des Videokopfes in Nanosekunden.
pub fn ntp_to_ns(timestamp: u64) -> u64 {
    let seconds = timestamp >> 32;
    let fraction = timestamp & 0xffff_ffff;
    seconds.wrapping_mul(1_000_000_000).wrapping_add((fraction * 1_000_000_000) >> 32)
}

#[derive(Default)]
struct Inner {
    video: Option<(bool, Pipeline)>,
    audio: Option<(u8, Pipeline)>,
    volume: f64,
    muted: bool,
    video_paused: bool,
    video_clock: VideoClock,
    audio_frames: u64,
}

const H264_CAPS: &str = "video/x-h264,stream-format=(string)byte-stream,alignment=(string)au";
const H265_CAPS: &str = "video/x-h265,stream-format=(string)byte-stream,alignment=(string)au";
const AAC_ELD_CAPS: &str =
    "audio/mpeg,mpegversion=(int)4,channels=(int)2,rate=(int)44100,stream-format=raw,codec_data=(buffer)f8e85000";
const AAC_LC_CAPS: &str =
    "audio/mpeg,mpegversion=(int)4,channels=(int)2,rate=(int)44100,stream-format=raw,codec_data=(buffer)1210";
const ALAC_CAPS: &str = "audio/x-alac,mpegversion=(int)4,channels=(int)2,rate=(int)44100,stream-format=raw,codec_data=(buffer)00000024616c616300000000000001600010280a0e0200ff00000000000000000000ac44";
const PCM_CAPS: &str = "audio/x-raw,rate=(int)44100,channels=(int)2,format=S16LE,layout=interleaved";

impl Media {
    pub fn new(video_sink: &str, audio_sink: &str, software_decoder: bool) -> Self {
        gst::on_frame_size(|w, h| control::emit(&format!("FRAME|{w}|{h}")));
        Self {
            video_sink: video_sink.to_owned(),
            audio_sink: audio_sink.to_owned(),
            software_decoder,
            inner: Mutex::new(Inner { volume: 1.0, ..Default::default() }),
        }
    }

    fn video_description(&self, h265: bool) -> String {
        let parser = if h265 { "h265parse" } else { "h264parse" };
        let decoder = match (self.software_decoder, h265) {
            (true, false) => "avdec_h264",
            (true, true) => "avdec_h265",
            _ => "decodebin",
        };
        let (element, options) = self.video_sink.split_once(' ').unwrap_or((self.video_sink.as_str(), ""));
        format!(
            "appsrc name=video_source ! queue ! {parser} ! {decoder} ! videoconvert ! videoscale ! {element} name=pcmirror_video {options} sync=true max-lateness=-1 qos=false"
        )
    }

    /// Codec festlegen (neues SPS/PPS). Startet die passende Pipeline bei Bedarf.
    pub fn set_video_codec(&self, h265: bool) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        if matches!(&inner.video, Some((current, _)) if *current == h265) {
            return Ok(());
        }
        inner.video = None;
        let caps = if h265 { H265_CAPS } else { H264_CAPS };
        let pipeline = Pipeline::launch(&self.video_description(h265), Some("video_source"), Some(caps))?;
        if let Some(sink) = pipeline.element("pcmirror_video") {
            sink.watch_frames();
        }
        pipeline.set_state(GST_STATE_PLAYING);
        control::log(&format!("Videoausgabe gestartet ({})", if h265 { "H.265" } else { "H.264" }));
        inner.video = Some((h265, pipeline));
        inner.video_paused = false;
        inner.video_clock.reset();
        Ok(())
    }

    /// `remote_ns`: Zeitstempel des Senders für dieses Bild.
    pub fn push_video(&self, data: &[u8], remote_ns: u64) {
        let mut inner = self.inner.lock().unwrap();
        let inner = &mut *inner;
        if let Some((_, pipeline)) = &inner.video {
            let pts = pipeline.running_time().map(|now| inner.video_clock.pts(remote_ns, now));
            pipeline.push_at(data, pts);
            if let Some(error) = pipeline.poll_error() {
                control::log(&format!("Videofehler: {error}"));
            }
        }
    }

    pub fn video_paused(&self, paused: bool) {
        self.inner.lock().unwrap().video_paused = paused;
    }

    /// Video beenden (Verbindung getrennt oder TEARDOWN).
    pub fn stop_video(&self) {
        let had_video = self.inner.lock().unwrap().video.take().is_some();
        gst::clear_last_frame();
        if had_video {
            control::emit("IDLE");
        }
    }

    pub fn start_audio(&self, ct: u8) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        if matches!(&inner.audio, Some((current, _)) if *current == ct) {
            return Ok(());
        }
        inner.audio = None;
        let (caps, decoder) = match ct {
            8 => (AAC_ELD_CAPS, "avdec_aac ! "),
            4 => (AAC_LC_CAPS, "avdec_aac ! "),
            2 => (ALAC_CAPS, "avdec_alac ! "),
            _ => (PCM_CAPS, ""),
        };
        let description = format!(
            "appsrc name=audio_source ! queue ! {decoder}audioconvert ! audioresample quality=10 ! volume name=volume ! level ! {} sync=false",
            self.audio_sink
        );
        let pipeline = Pipeline::launch(&description, Some("audio_source"), Some(caps))?;
        if let Some(volume) = pipeline.element("volume") {
            volume.set_f64("volume", inner.volume);
            volume.set_bool("mute", inner.muted);
        }
        pipeline.set_state(GST_STATE_PLAYING);
        inner.audio = Some((ct, pipeline));
        inner.audio_frames = 0;
        Ok(())
    }

    /// Prüft das erste Byte wie die C-Fassung, damit kaputte Frames nicht in den Decoder gehen.
    pub fn push_audio(&self, frame: &[u8]) {
        let mut inner = self.inner.lock().unwrap();
        inner.audio_frames += 1;
        let first_frame = inner.audio_frames == 1;
        let Some((ct, pipeline)) = &inner.audio else { return };
        let Some(&lead) = frame.first() else { return };
        let valid = match ct {
            8 => matches!(lead, 0x8c | 0x8d | 0x8e | 0x80 | 0x81 | 0x82),
            2 => lead == 0x20,
            4 => lead == 0xff,
            _ => true,
        };
        if valid {
            pipeline.push(frame);
            if first_frame {
                control::log(&format!("Ton läuft ({})", match ct { 8 => "AAC-ELD", 4 => "AAC", 2 => "ALAC", _ => "PCM" }));
            }
        } else if first_frame {
            control::log(&format!("Ton: ungültiger Frame verworfen (erstes Byte {lead:#04x})"));
        }
        if let Some(error) = pipeline.poll_error() {
            control::log(&format!("Tonfehler: {error}"));
        }
    }

    pub fn stop_audio(&self) {
        self.inner.lock().unwrap().audio = None;
    }

    /// AirPlay-Lautstärke in dB (-30…0, -144 = stumm), umgerechnet wie die C-Fassung.
    pub fn set_airplay_volume(&self, db: f32) {
        let linear = if db <= -144.0 || db <= -30.0 {
            0.0
        } else {
            10f64.powf(0.05 * f64::from(db.min(0.0)))
        };
        let mut inner = self.inner.lock().unwrap();
        inner.volume = linear;
        if let Some((_, p)) = &inner.audio {
            if let Some(v) = p.element("volume") {
                v.set_f64("volume", linear);
            }
        }
    }

    pub fn set_mute(&self, muted: bool) {
        let mut inner = self.inner.lock().unwrap();
        inner.muted = muted;
        if let Some((_, p)) = &inner.audio {
            if let Some(v) = p.element("volume") {
                v.set_bool("mute", muted);
            }
        }
    }

    pub fn stop_all(&self) {
        self.stop_audio();
        self.stop_video();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_clock_paces_by_sender_time() {
        let mut clock = VideoClock::default();
        let ms = 1_000_000u64;
        // erstes Bild (Senderzeit 1000 ms) kommt bei Laufzeit 100 ms an
        assert_eq!(clock.pts(1_000 * ms, 100 * ms), 140 * ms);
        // nächstes Bild 33 ms später erzeugt, aber im Schub erst 60 ms später angekommen
        assert_eq!(clock.pts(1_033 * ms, 160 * ms), 173 * ms);
        // kurz danach im selben Schub: Takt bleibt 33 ms
        assert_eq!(clock.pts(1_066 * ms, 170 * ms), 206 * ms);
        // schnellere Übertragung als beim ersten Bild: Bezug wird nachgezogen
        assert_eq!(clock.pts(1_100 * ms, 190 * ms), 230 * ms);
        // 1 s Verspätung: neu ausrichten statt dauerhaft hinterherzulaufen
        assert_eq!(clock.pts(1_133 * ms, 2_000 * ms), 2_040 * ms);
    }

    #[test]
    fn ntp_conversion() {
        assert_eq!(ntp_to_ns(1 << 32), 1_000_000_000);
        assert_eq!(ntp_to_ns((2 << 32) | 0x8000_0000), 2_500_000_000);
    }

    #[test]
    fn volume_mapping() {
        let to_linear = |db: f32| if db <= -30.0 { 0.0 } else { 10f64.powf(0.05 * f64::from(db.min(0.0))) };
        assert_eq!(to_linear(-144.0), 0.0);
        assert_eq!(to_linear(0.0), 1.0);
        assert!((to_linear(-6.0) - 0.501).abs() < 0.01);
    }
}

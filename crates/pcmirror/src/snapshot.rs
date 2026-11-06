// SPDX-License-Identifier: GPL-3.0-or-later
//! Wandelt den von UxPlay geschriebenen PPM-Frame (P6) in eine PNG-Datei um.
use std::fs;
use std::io::BufWriter;
use std::path::Path;

pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub fn read_ppm(data: &[u8]) -> Result<Rgb, String> {
    let mut fields = Vec::new();
    let mut position = 0;
    while fields.len() < 4 {
        while position < data.len() && data[position].is_ascii_whitespace() {
            position += 1;
        }
        if data.get(position) == Some(&b'#') {
            while position < data.len() && data[position] != b'\n' {
                position += 1;
            }
            continue;
        }
        let start = position;
        while position < data.len() && !data[position].is_ascii_whitespace() {
            position += 1;
        }
        if start == position {
            return Err("PPM-Kopf unvollständig".into());
        }
        fields.push(
            std::str::from_utf8(&data[start..position])
                .map_err(|e| e.to_string())?
                .to_owned(),
        );
    }
    position += 1; // genau ein Trennzeichen vor den Binärdaten
    if fields[0] != "P6" || fields[3] != "255" {
        return Err("Nur PPM P6 mit 8 Bit wird unterstützt".into());
    }
    let width: u32 = fields[1].parse().map_err(|_| "Breite ungültig")?;
    let height: u32 = fields[2].parse().map_err(|_| "Höhe ungültig")?;
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err("Bildgröße ungültig".into());
    }
    let expected = width as usize * height as usize * 3;
    let pixels = data
        .get(position..position + expected)
        .ok_or("PPM-Daten unvollständig")?
        .to_vec();
    Ok(Rgb {
        width,
        height,
        pixels,
    })
}

pub fn ppm_to_png(source: &Path, target: &Path) -> Result<(), String> {
    let data = fs::read(source).map_err(|e| e.to_string())?;
    let image = read_ppm(&data)?;
    let temporary = target.with_extension("png.partial");
    let file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let result = encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&image.pixels))
        .map_err(|e| e.to_string());
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, target).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_ppm() {
        let mut data = b"P6\n# Kommentar\n2 1\n255\n".to_vec();
        data.extend_from_slice(&[255, 0, 0, 0, 0, 255]);
        let image = read_ppm(&data).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.pixels, vec![255, 0, 0, 0, 0, 255]);

        let folder = std::env::temp_dir().join(format!("pcmirror-test-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let source = folder.join("bild ä.ppm");
        let target = folder.join("bild ä.png");
        fs::write(&source, &data).unwrap();
        ppm_to_png(&source, &target).unwrap();
        assert!(fs::read(&target).unwrap().starts_with(b"\x89PNG"));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn rejects_truncated() {
        assert!(read_ppm(b"P6\n2 2\n255\n\x00").is_err());
        assert!(read_ppm(b"P3\n1 1\n255\n").is_err());
    }
}

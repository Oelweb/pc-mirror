// SPDX-License-Identifier: GPL-3.0-or-later
//! Kleine Geheimnisse (PIN) mit Windows-DPAPI an das Benutzerkonto binden,
//! statt sie im Klartext in settings.ini abzulegen.
use base64::{engine::general_purpose::STANDARD, Engine};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

const ENTROPY: &[u8] = b"PC-Mirror receiver PIN";

pub fn protect(plain: &str) -> Result<String, String> {
    let output = transform(plain.as_bytes(), true)?;
    Ok(STANDARD.encode(output))
}

pub fn unprotect(encoded: &str) -> Result<String, String> {
    let data = STANDARD.decode(encoded.trim()).map_err(|e| e.to_string())?;
    let output = transform(&data, false)?;
    String::from_utf8(output).map_err(|e| e.to_string())
}

fn transform(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let input_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if encrypt {
            CryptProtectData(
                &input_blob,
                None,
                Some(&entropy),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input_blob,
                None,
                Some(&entropy),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|e| e.to_string())?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(output.pbData as _)));
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        let protected = super::protect("4711").unwrap();
        assert!(!protected.contains("4711"));
        assert_eq!(super::unprotect(&protected).unwrap(), "4711");
    }
}

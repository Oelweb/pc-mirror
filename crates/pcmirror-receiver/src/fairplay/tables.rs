// SPDX-License-Identifier: GPL-3.0-or-later
//! Tabellen aus UxPlays `playfair/omg_hax.h`, unverändert als Binärdaten
//! (Bytes bzw. 32-Bit-Wörter in Little Endian), erzeugt per Skript aus dem C-Header.

const fn words<const N: usize>(bytes: &[u8]) -> [u32; N] {
    let mut out = [0u32; N];
    let mut i = 0;
    while i < N {
        out[i] = u32::from_le_bytes([bytes[4 * i], bytes[4 * i + 1], bytes[4 * i + 2], bytes[4 * i + 3]]);
        i += 1;
    }
    out
}

pub static MESSAGE_KEY: &[u8; 576] = include_bytes!("tables/message_key.bin");
pub static MESSAGE_IV: &[u8; 64] = include_bytes!("tables/message_iv.bin");
pub static Z_KEY: &[u8; 16] = include_bytes!("tables/z_key.bin");
pub static X_KEY: &[u8; 16] = include_bytes!("tables/x_key.bin");
pub static T_KEY: &[u8; 16] = include_bytes!("tables/t_key.bin");
pub static TABLE_S1: &[u8; 10240] = include_bytes!("tables/table_s1.bin");
pub static TABLE_S2: &[u8; 36864] = include_bytes!("tables/table_s2.bin");
pub static TABLE_S3: &[u8; 4096] = include_bytes!("tables/table_s3.bin");
pub static TABLE_S4: &[u8; 36864] = include_bytes!("tables/table_s4.bin");
pub static TABLE_S10: &[u8; 4096] = include_bytes!("tables/table_s10.bin");
pub static TABLE_S5: [u32; 256] = words(include_bytes!("tables/table_s5.bin"));
pub static TABLE_S6: [u32; 256] = words(include_bytes!("tables/table_s6.bin"));
pub static TABLE_S7: [u32; 256] = words(include_bytes!("tables/table_s7.bin"));
pub static TABLE_S8: [u32; 256] = words(include_bytes!("tables/table_s8.bin"));
pub static TABLE_S9: [u32; 1024] = words(include_bytes!("tables/table_s9.bin"));

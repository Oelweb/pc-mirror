// SPDX-License-Identifier: GPL-3.0-or-later
//! Rust-Port von UxPlays `playfair` (omg_hax.c, hand_garble.c, modified_md5.c, sap_hash.c).
//!
//! Die Originale rechnen mit C-Integer-Promotion: Alles hier läuft in `u32` mit
//! Überlauf-Arithmetik; nur Division, Modulo und Rechtsverschiebung hängen vom
//! Vorzeichen ab, und an diesen Stellen sind die Operanden im Original nie negativ
//! (geprüft und per Vergleichstest gegen die C-Fassung abgesichert).
#![allow(clippy::needless_range_loop, clippy::too_many_lines)]

use super::tables::*;

type Schedule = [[u32; 4]; 11];

fn le_words(bytes: &[u8; 16]) -> [u32; 4] {
    std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}

fn put_le_words(bytes: &mut [u8; 16], words: [u32; 4]) {
    for (i, w) in words.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
}

fn z_xor(block: &mut [u8]) {
    for (i, b) in block.iter_mut().enumerate() {
        *b ^= Z_KEY[i % 16];
    }
}

fn x_xor(block: &mut [u8]) {
    for (i, b) in block.iter_mut().enumerate() {
        *b ^= X_KEY[i % 16];
    }
}

const SAP_IV: [u8; 16] = [
    0x2B, 0x84, 0xFB, 0x79, 0xDA, 0x75, 0xB9, 0x04, 0x6C, 0x24, 0x73, 0xF7, 0xD1, 0xC4, 0xAB, 0x0E,
];
const SAP_KEY_MATERIAL: [u8; 16] = [
    0xA1, 0x1A, 0x4A, 0x83, 0xF2, 0x7A, 0x75, 0xEE, 0xA2, 0x1A, 0x7D, 0xB8, 0x8D, 0x77, 0x92, 0xAB,
];
const INDEX_MANGLE: [u8; 11] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36, 0x6C];
const INITIAL_SESSION_KEY: [u8; 16] = [
    0xDC, 0xDC, 0xF3, 0xB9, 0x0B, 0x74, 0xDC, 0xFB, 0x86, 0x7F, 0xF7, 0x60, 0x16, 0x72, 0x90, 0x51,
];
const STATIC_SOURCE_1: [u8; 17] = [
    0xFA, 0x9C, 0xAD, 0x4D, 0x4B, 0x68, 0x26, 0x8C, 0x7F, 0xF3, 0x88, 0x99, 0xDE, 0x92, 0x2E, 0x95,
    0x1E,
];
const STATIC_SOURCE_2: [u8; 47] = [
    0xEC, 0x4E, 0x27, 0x5E, 0xFD, 0xF2, 0xE8, 0x30, 0x97, 0xAE, 0x70, 0xFB, 0xE0, 0x00, 0x3F, 0x1C,
    0x39, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
const DEFAULT_SAP: [u8; 0x114] = [
    0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79,
    0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79,
    0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79,
    0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79, 0x79,
    0x79, 0x79, 0x79, 0x79, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x02, 0x53,
    0x00, 0x01, 0xcc, 0x34, 0x2a, 0x5e, 0x5b, 0x1a, 0x67, 0x73, 0xc2, 0x0e, 0x21, 0xb8, 0x22, 0x4d,
    0xf8, 0x62, 0x48, 0x18, 0x64, 0xef, 0x81, 0x0a, 0xae, 0x2e, 0x37, 0x03, 0xc8, 0x81, 0x9c, 0x23,
    0x53, 0x9d, 0xe5, 0xf5, 0xd7, 0x49, 0xbc, 0x5b, 0x7a, 0x26, 0x6c, 0x49, 0x62, 0x83, 0xce, 0x7f,
    0x03, 0x93, 0x7a, 0xe1, 0xf6, 0x16, 0xde, 0x0c, 0x15, 0xff, 0x33, 0x8c, 0xca, 0xff, 0xb0, 0x9e,
    0xaa, 0xbb, 0xe4, 0x0f, 0x5d, 0x5f, 0x55, 0x8f, 0xb9, 0x7f, 0x17, 0x31, 0xf8, 0xf7, 0xda, 0x60,
    0xa0, 0xec, 0x65, 0x79, 0xc3, 0x3e, 0xa9, 0x83, 0x12, 0xc3, 0xb6, 0x71, 0x35, 0xa6, 0x69, 0x4f,
    0xf8, 0x23, 0x05, 0xd9, 0xba, 0x5c, 0x61, 0x5f, 0xa2, 0x54, 0xd2, 0xb1, 0x83, 0x45, 0x83, 0xce,
    0xe4, 0x2d, 0x44, 0x26, 0xc8, 0x35, 0xa7, 0xa5, 0xf6, 0xc8, 0x42, 0x1c, 0x0d, 0xa3, 0xf1, 0xc7,
    0x00, 0x50, 0xf2, 0xe5, 0x17, 0xf8, 0xd0, 0xfa, 0x77, 0x8d, 0xfb, 0x82, 0x8d, 0x40, 0xc7, 0x8e,
    0x94, 0x1e, 0x1e, 0x1e,
];

fn table_index(i: usize) -> &'static [u8] {
    let start = ((31 * i) % 0x28) << 8;
    &TABLE_S1[start..start + 256]
}

fn message_table_index(i: usize) -> &'static [u8] {
    let start = ((97 * i) % 144) << 8;
    &TABLE_S2[start..start + 256]
}

fn permute_table_2(i: usize) -> &'static [u8] {
    let start = ((71 * i) % 144) << 8;
    &TABLE_S4[start..start + 256]
}

/// Gemeinsames Muster aller drei Permutationen (ShiftRows mit Tabellen).
fn permute(block: &mut [u8; 16], table: impl Fn(usize) -> &'static [u8]) {
    block[0] = table(0)[block[0] as usize];
    block[4] = table(4)[block[4] as usize];
    block[8] = table(8)[block[8] as usize];
    block[12] = table(12)[block[12] as usize];

    let tmp = block[13];
    block[13] = table(13)[block[9] as usize];
    block[9] = table(9)[block[5] as usize];
    block[5] = table(5)[block[1] as usize];
    block[1] = table(1)[tmp as usize];

    let tmp = block[2];
    block[2] = table(2)[block[10] as usize];
    block[10] = table(10)[tmp as usize];
    let tmp = block[6];
    block[6] = table(6)[block[14] as usize];
    block[14] = table(14)[tmp as usize];

    let tmp = block[3];
    block[3] = table(3)[block[7] as usize];
    block[7] = table(7)[block[11] as usize];
    block[11] = table(11)[block[15] as usize];
    block[15] = table(15)[tmp as usize];
}

fn permute_block_1(block: &mut [u8; 16]) {
    // table_s3[0x100*k + x] mit der Zuordnung aus omg_hax.c
    const PAGE: [usize; 16] = [0x0, 0x5, 0xa, 0xf, 0x4, 0x9, 0xe, 0x3, 0x8, 0xd, 0x2, 0x7, 0xc, 0x1, 0x6, 0xb];
    permute(block, |pos| {
        let start = PAGE[pos] << 8;
        &TABLE_S3[start..start + 256]
    });
}

fn permute_block_2(block: &mut [u8; 16], round: usize) {
    permute(block, |pos| permute_table_2(round * 16 + pos));
}

fn generate_key_schedule(key_material: &[u8; 16]) -> Schedule {
    let mut schedule = [[0xdead_beefu32; 4]; 11];
    let mut buffer = [0u8; 16];
    for i in 0..16 {
        buffer[i] = key_material[i] ^ T_KEY[i];
    }
    let mut ti = 0;
    for round in 0..11 {
        let mut data = le_words(&buffer);
        schedule[round][0] = data[0];
        let t1 = table_index(ti);
        let t2 = table_index(ti + 1);
        let t3 = table_index(ti + 2);
        let t4 = table_index(ti + 3);
        ti += 4;
        buffer[0] ^= t1[buffer[0x0d] as usize] ^ INDEX_MANGLE[round];
        buffer[1] ^= t2[buffer[0x0e] as usize];
        buffer[2] ^= t3[buffer[0x0f] as usize];
        buffer[3] ^= t4[buffer[0x0c] as usize];
        data[0] = le_words(&buffer)[0];
        schedule[round][1] = data[1];
        data[1] ^= data[0];
        schedule[round][2] = data[2];
        data[2] ^= data[1];
        schedule[round][3] = data[3];
        data[3] ^= data[2];
        put_le_words(&mut buffer, data);
    }
    schedule
}

fn t_lookup(table: &[u32; 256], index: u8) -> u32 {
    table[index as usize]
}

fn cycle(block: &mut [u8; 16], schedule: &Schedule) {
    let mut words = le_words(block);
    for i in 0..4 {
        words[i] ^= schedule[10][i];
    }
    put_le_words(block, words);
    permute_block_1(block);

    for round in 0..9 {
        let keys = schedule[9 - round];
        let k0 = keys[0].to_le_bytes();
        let ab = t_lookup(&TABLE_S5, block[3] ^ k0[3])
            ^ t_lookup(&TABLE_S6, block[2] ^ k0[2])
            ^ t_lookup(&TABLE_S8, block[0] ^ k0[0])
            ^ t_lookup(&TABLE_S7, block[1] ^ k0[1]);
        block[0..4].copy_from_slice(&ab.to_le_bytes());

        let k1 = keys[1].to_le_bytes();
        let ab = t_lookup(&TABLE_S6, block[6] ^ k1[2])
            ^ t_lookup(&TABLE_S5, block[7] ^ k1[3])
            ^ t_lookup(&TABLE_S8, block[4] ^ k1[0])
            ^ t_lookup(&TABLE_S7, block[5] ^ k1[1]);
        let k2 = keys[2].to_le_bytes();
        let k3 = keys[3].to_le_bytes();
        block[4..8].copy_from_slice(&ab.to_le_bytes());
        let w2 = t_lookup(&TABLE_S5, block[11] ^ k2[3])
            ^ t_lookup(&TABLE_S6, block[10] ^ k2[2])
            ^ t_lookup(&TABLE_S7, block[9] ^ k2[1])
            ^ t_lookup(&TABLE_S8, block[8] ^ k2[0]);
        block[8..12].copy_from_slice(&w2.to_le_bytes());
        let w3 = t_lookup(&TABLE_S5, block[15] ^ k3[3])
            ^ t_lookup(&TABLE_S6, block[14] ^ k3[2])
            ^ t_lookup(&TABLE_S7, block[13] ^ k3[1])
            ^ t_lookup(&TABLE_S8, block[12] ^ k3[0]);
        block[12..16].copy_from_slice(&w3.to_le_bytes());
        permute_block_2(block, 8 - round);
    }
    let mut words = le_words(block);
    for i in 0..4 {
        words[i] ^= schedule[0][i];
    }
    put_le_words(block, words);
}

/// Entschlüsselt die 128 Byte der Client-Nachricht. `mode` ist bereits geprüft (0..=3).
fn decrypt_message(message_in: &[u8; 164], mode: usize) -> [u8; 128] {
    let mut decrypted = [0u8; 128];
    let message_key = &MESSAGE_KEY[mode * 144..mode * 144 + 144];
    let message_iv = &MESSAGE_IV[mode * 16..mode * 16 + 16];
    for i in 0..8 {
        let mut buffer = [0u8; 16];
        for j in 0..16 {
            buffer[j] = if mode == 3 {
                message_in[(0x80 - 0x10 * i) + j]
            } else {
                message_in[(0x10 * (i + 1)) + j]
            };
        }
        for j in 0..9 {
            let base = 0x80 - 0x10 * j;
            permute(&mut buffer, |pos| message_table_index(base + pos));
            // permute() liest dieselben Indizes; die XOR-Schlüssel kommen danach.
            for pos in 0..16 {
                buffer[pos] ^= message_key[base + pos];
            }
            let b = buffer;
            let s9 = |page: usize, x: u8| TABLE_S9[page * 0x100 + x as usize];
            let words = [
                s9(0, b[0x0]) ^ s9(1, b[0x1]) ^ s9(2, b[0x2]) ^ s9(3, b[0x3]),
                s9(0, b[0x4]) ^ s9(1, b[0x5]) ^ s9(2, b[0x6]) ^ s9(3, b[0x7]),
                s9(0, b[0x8]) ^ s9(1, b[0x9]) ^ s9(2, b[0xa]) ^ s9(3, b[0xb]),
                s9(0, b[0xc]) ^ s9(1, b[0xd]) ^ s9(2, b[0xe]) ^ s9(3, b[0xf]),
            ];
            put_le_words(&mut buffer, words);
        }
        permute(&mut buffer, |pos| {
            let start = pos << 8;
            &TABLE_S10[start..start + 256]
        });
        if mode <= 2 {
            let previous: &[u8] = if i > 0 { &message_in[0x10 * i..0x10 * i + 16] } else { message_iv };
            for k in 0..16 {
                decrypted[0x10 * i + k] = buffer[k] ^ previous[k];
            }
        } else {
            let at = 0x70 - 0x10 * i;
            let previous: &[u8] = if i < 7 { &message_in[at..at + 16] } else { message_iv };
            for k in 0..16 {
                decrypted[at + k] = buffer[k] ^ previous[k];
            }
        }
    }
    decrypted
}

// ----------------------------------------------------------------- modified MD5

const MD5_SHIFT: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// `(int)(long long)((1LL << 32) * fabs(sin(i + 1)))` – die MD5-Konstanten, fest
/// hinterlegt statt zur Laufzeit über `sin()` berechnet.
const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

fn md5_constant(i: usize) -> u32 {
    MD5_K[i]
}

fn modified_md5(original_block: &[u8], key_in: &[u8; 16]) -> [u8; 16] {
    let mut block = [0u8; 64];
    block.copy_from_slice(&original_block[..64]);
    let key = le_words(key_in);
    let (mut a, mut b, mut c, mut d) = (key[0], key[1], key[2], key[3]);
    let word = |block: &[u8; 64], j: usize| {
        u32::from_le_bytes(block[4 * j..4 * j + 4].try_into().unwrap())
    };
    let set_word = |block: &mut [u8; 64], j: usize, v: u32| {
        block[4 * j..4 * j + 4].copy_from_slice(&v.to_le_bytes());
    };
    for i in 0..64 {
        let j = if i < 16 {
            i
        } else if i < 32 {
            (5 * i + 1) % 16
        } else if i < 48 {
            (3 * i + 5) % 16
        } else {
            7 * i % 16
        };
        let input = (block[4 * j] as u32) << 24
            | (block[4 * j + 1] as u32) << 16
            | (block[4 * j + 2] as u32) << 8
            | block[4 * j + 3] as u32;
        let mut z = a.wrapping_add(input).wrapping_add(md5_constant(i));
        let f = if i < 16 {
            (b & c) | (!b & d)
        } else if i < 32 {
            (b & d) | (c & !d)
        } else if i < 48 {
            b ^ c ^ d
        } else {
            c ^ (b | !d)
        };
        z = z.wrapping_add(f).rotate_left(MD5_SHIFT[i]);
        z = z.wrapping_add(b);
        let tmp = d;
        d = c;
        c = b;
        b = z;
        a = tmp;
        if i == 31 {
            let mut swap = |x: usize, y: usize| {
                let (vx, vy) = (word(&block, x), word(&block, y));
                set_word(&mut block, x, vy);
                set_word(&mut block, y, vx);
            };
            swap((a & 15) as usize, (b & 15) as usize);
            swap((c & 15) as usize, (d & 15) as usize);
            swap(((a & (15 << 4)) >> 4) as usize, ((b & (15 << 4)) >> 4) as usize);
            swap(((a & (15 << 8)) >> 8) as usize, ((b & (15 << 8)) >> 8) as usize);
            swap(((a & (15 << 12)) >> 12) as usize, ((b & (15 << 12)) >> 12) as usize);
        }
    }
    let mut out = [0u8; 16];
    put_le_words(
        &mut out,
        [
            key[0].wrapping_add(a),
            key[1].wrapping_add(b),
            key[2].wrapping_add(c),
            key[3].wrapping_add(d),
        ],
    );
    out
}

// ------------------------------------------------------------------- SAP-Hash

fn rol8(input: u8, count: u32) -> u8 {
    input.rotate_left(count % 8)
}

/// `rol8x` ohne Maskierung (Ergebnis kann über 255 liegen).
fn rol8x(input: u8, count: u32) -> u32 {
    ((input as u32) << count) | ((input as u32) >> (8 - count))
}

fn weird_ror8(input: u8, count: u32) -> u32 {
    if count == 0 {
        return 0;
    }
    (((input as u32) >> count) & 0xff) | ((input as u32) << (8 - count))
}

fn weird_rol8(input: u8, count: u32) -> u32 {
    if count == 0 {
        return 0;
    }
    (((input as u32) << count) & 0xff) | ((input as u32) >> (8 - count))
}

fn weird_rol32(input: u8, count: u32) -> u32 {
    if count == 0 {
        return 0;
    }
    ((input as u32) << count) ^ ((input as u32) >> (8 - count))
}

fn sap_hash(block_in: &[u8], key_out: &mut [u8; 16]) {
    let mut buffer0: [u8; 20] = [
        0x96, 0x5F, 0xC6, 0x53, 0xF8, 0x46, 0xCC, 0x18, 0xDF, 0xBE, 0xB2, 0xF8, 0x38, 0xD7, 0xEC,
        0x22, 0x03, 0xD1, 0x20, 0x8F,
    ];
    let mut buffer1 = [0u8; 210];
    let mut buffer2: [u8; 35] = [
        0x43, 0x54, 0x62, 0x7A, 0x18, 0xC3, 0xD6, 0xB3, 0x9A, 0x56, 0xF6, 0x1C, 0x14, 0x3F, 0x0C,
        0x1D, 0x3B, 0x36, 0x83, 0xB1, 0x39, 0x51, 0x4A, 0xAA, 0x09, 0x3E, 0xFE, 0x44, 0xAF, 0xDE,
        0xC3, 0x20, 0x9D, 0x42, 0x3A,
    ];
    let mut buffer3 = [0u8; 132];
    let mut buffer4: [u8; 21] = [
        0xED, 0x25, 0xD1, 0xBB, 0xBC, 0x27, 0x9F, 0x02, 0xA2, 0xA9, 0x11, 0x00, 0x0C, 0xB3, 0x52,
        0xC0, 0xBD, 0xE3, 0x1B, 0x49, 0xC7,
    ];
    const I0_INDEX: [usize; 11] = [18, 22, 23, 0, 5, 19, 32, 31, 10, 21, 30];

    for i in 0..210usize {
        let word_index = (i % 64) >> 2;
        let in_word = u32::from_le_bytes(block_in[word_index * 4..word_index * 4 + 4].try_into().unwrap());
        buffer1[i] = ((in_word >> ((3 - (i % 4)) << 3)) & 0xff) as u8;
    }
    let wrap_index = |i: usize, back: usize, modulo: usize| -> usize {
        ((i as i64 - back as i64) as u32 as usize) % modulo
    };
    for i in 0..840usize {
        let x = buffer1[wrap_index(i, 155, 210)];
        let y = buffer1[wrap_index(i, 57, 210)];
        let z = buffer1[wrap_index(i, 13, 210)];
        let w = buffer1[i % 210];
        buffer1[i % 210] = rol8(y, 5)
            .wrapping_add(rol8(z, 3) ^ w)
            .wrapping_sub(rol8(x, 7));
    }
    garble(&mut buffer0, &mut buffer1, &mut buffer2, &mut buffer3, &mut buffer4);

    key_out.fill(0xE1);
    for i in 0..11 {
        if i == 3 {
            key_out[i] = 0x3d;
        } else {
            key_out[i] = key_out[i].wrapping_add(buffer3[I0_INDEX[i] * 4]);
        }
    }
    for i in 0..20 {
        key_out[i % 16] ^= buffer0[i];
    }
    for i in 0..35 {
        key_out[i % 16] ^= buffer2[i];
    }
    for i in 0..210 {
        key_out[i % 16] ^= buffer1[i];
    }
    for _ in 0..16 {
        for i in 0..16usize {
            let x = key_out[wrap_index(i, 7, 16)];
            let y = key_out[i % 16];
            let z = key_out[wrap_index(i, 37, 16)];
            let w = key_out[wrap_index(i, 177, 16)];
            key_out[i] = rol8(x, 1) ^ y ^ rol8(z, 6) ^ rol8(w, 5);
        }
    }
}

/// hand_garble.c. Alle Zwischenwerte als `u32` mit Überlauf (wie C `unsigned int`
/// bzw. `int` im Zweierkomplement); Zuweisungen an Puffer kürzen auf 8 Bit.
#[allow(non_snake_case, unused_assignments, unused_mut, unused_macros)]
fn garble(b0: &mut [u8; 20], b1: &mut [u8; 210], b2: &mut [u8; 35], b3: &mut [u8; 132], b4: &mut [u8; 21]) {
    macro_rules! u {
        ($e:expr) => {
            ($e) as u32
        };
    }
    // Index-Hilfen: buffer[x % n] mit x als u32
    macro_rules! i0 { ($e:expr) => { b0[(u!($e) % 20) as usize] as u32 }; }
    macro_rules! i1 { ($e:expr) => { b1[(u!($e) % 210) as usize] as u32 }; }
    macro_rules! i2 { ($e:expr) => { b2[(u!($e) % 35) as usize] as u32 }; }
    macro_rules! i4 { ($e:expr) => { b4[(u!($e) % 21) as usize] as u32 }; }
    let w = |x: u32| x as u8; // Kürzen auf unsigned char
    let mut tmp: u32;
    let tmp2: u32;
    let mut tmp3: u32 = 0;
    let (mut A, mut B, mut C, mut D, mut E, mut M, mut J, mut G, mut F, mut H, mut K);
    let (R, S, T, U, V, W, X, Y, Z);

    b2[12] = w(0x14u32.wrapping_add(
        ((u!(b1[64]) & 92) | ((u!(b1[99]) / 3) & 35))
            & i4!(rol8x(b4[(u!(b1[206]) % 21) as usize], 4)),
    ));
    b1[4] = w((u!(b1[99]) / 5).wrapping_mul(u!(b1[99]) / 5).wrapping_mul(2));
    b2[34] = 0xb8;
    {
        let x = i2!(b1[203]);
        b1[153] ^= w(x.wrapping_mul(x).wrapping_mul(u!(b1[190])));
    }
    b0[3] = w(u!(b0[3]).wrapping_sub(((i4!(b1[205]) >> 1) & 80) | 0xe6440));
    b0[16] = 0x93;
    b0[13] = 0x62;
    b1[33] = w(u!(b1[33]).wrapping_sub(i4!(b1[36]) & 0xf6));
    tmp2 = i2!(b1[67]);
    b2[12] = 0x07;
    tmp = i0!(b1[181]);
    b1[2] = w(u!(b1[2]).wrapping_sub(3136));
    b0[19] = w(i4!(b1[58]));
    b3[0] = w(92u32.wrapping_sub(i2!(b1[32])));
    b3[4] = w(i2!(b1[15]).wrapping_add(0x9e));
    b1[34] = w(u!(b1[34]).wrapping_add(i4!((i2!(b1[15]).wrapping_add(0x9e)) & 0xff) / 5));
    b0[19] = w(u!(b0[19]).wrapping_add(0xfffffee6u32.wrapping_sub((i0!(b3[4]) >> 1) & 102)));
    {
        // (3*(((b1[72] >> (x & 7)) ^ (b1[72] << ((7 - (x-1)) & 7))) - (3*y))) ^ b1[15]
        let x = i4!(b1[190]);
        let shl = (7u32.wrapping_sub(x.wrapping_sub(1))) & 7;
        let inner = (u!(b1[72]) >> (x & 7)) ^ (u!(b1[72]) << shl);
        let v = 3u32.wrapping_mul(inner.wrapping_sub(3u32.wrapping_mul(i4!(b1[126]))));
        b1[15] = w(v ^ u!(b1[15]));
    }
    {
        let x = i2!(b1[181]);
        b0[15] ^= w(x.wrapping_mul(x).wrapping_mul(x));
    }
    b2[4] ^= w(u!(b1[202]) / 3);
    A = 92u32.wrapping_sub(i0!(b3[0]));
    E = (A & 0xc6) | (!u!(b1[105]) & 0xc6) | (A & !u!(b1[105]));
    b2[1] = w(u!(b2[1]).wrapping_add(E.wrapping_mul(E).wrapping_mul(E)));
    b0[19] ^= w(((224 | (i4!(b1[92]) & 27)).wrapping_mul(i2!(b1[41]))) / 3);
    b1[140] = w(u!(b1[140]).wrapping_add(weird_ror8(92, u!(b1[5]) & 7)));
    {
        let nb = !u!(b1[4]) ^ i2!(b1[12]);
        b2[12] = w(u!(b2[12]).wrapping_add(((nb | u!(b1[182])) & 192) | (nb & u!(b1[182]))));
    }
    b1[36] = w(u!(b1[36]).wrapping_add(125));
    {
        let p = (74 & u!(b1[138])) | ((74 | u!(b1[138])) & u!(b0[15]));
        let q = i0!(b1[43]);
        b1[124] = w(rol8x(w((p & q) | ((p | q) & 95)), 4));
    }
    b3[8] = w((((i0!(b3[4]) & 95) & ((i4!(b1[68]) & 46) << 1)) | 16) ^ 92);
    A = u!(b1[177]).wrapping_add(i4!(b1[79]));
    D = (((A >> 1) | ((3 * u!(b1[148])) / 5)) & u!(b2[1])) | ((A >> 1) & ((3 * u!(b1[148])) / 5));
    b3[12] = w((-34i32 as u32).wrapping_sub(D));
    A = 8u32.wrapping_sub(u!(b2[22]) & 7);
    B = u!(b1[33]) >> (A & 7);
    C = u!(b1[33]) << (u!(b2[22]) & 7);
    b2[16] = w(u!(b2[16]).wrapping_add(
        ((i2!(b3[0]) & 159) | i0!(b3[4]) | 8).wrapping_sub((B ^ C) | 128),
    ));
    b0[14] ^= w(i2!(b3[12]));
    A = weird_rol8(w(i4!(i0!(b1[201]))), (i2!(b1[112]) << 1) & 7);
    D = (i0!(b1[208]) & 131) | (i0!(b1[164]) & 124);
    b1[19] = w(u!(b1[19]).wrapping_add((A & (D / 5)) | ((A | (D / 5)) & 37)));
    {
        let x = i4!(b1[45]).wrapping_add(92);
        b2[8] = w(weird_ror8(140, x.wrapping_mul(x) & 7));
    }
    b1[190] = 56;
    b2[8] ^= b3[0];
    b1[53] = w(!((i0!(b1[83]) | 204) / 5));
    b0[13] = w(u!(b0[13]).wrapping_add(i0!(b1[41])));
    {
        let x = i2!(b3[0]);
        b0[10] = w(((x & u!(b1[2])) | ((x | u!(b1[2])) & u!(b3[12]))) / 15);
    }
    {
        let p = i4!(b1[2]) & 68;
        let q = i2!(b3[8]);
        A = (((56 | p) | q) & 42) | ((p | 56) & q);
    }
    b3[16] = w(A.wrapping_mul(A).wrapping_add(110));
    b3[20] = w(202u32.wrapping_sub(u!(b3[16])));
    b3[24] = b1[151];
    b2[13] ^= w(i4!(b3[0]));
    {
        let x = i2!(b1[179]).wrapping_sub(38);
        B = (x & 177) | (u!(b3[12]) & 177);
        C = x & u!(b3[12]);
    }
    b3[28] = w(30u32.wrapping_add((B | C).wrapping_mul(B | C)));
    b3[32] = w(u!(b3[28]).wrapping_add(62));
    {
        let s = u!(b3[20]).wrapping_add(u!(b3[0]) & 74);
        let n = !i4!(b3[0]);
        A = (s | n) & 121;
        B = s & n;
    }
    tmp3 = A | B;
    C = (((tmp3 ^ 0xffffffa6) | u!(b3[0])) & 4) | ((tmp3 ^ 0xffffffa6) & u!(b3[0]));
    b1[47] = w(i2!(b1[89]).wrapping_add(C) ^ u!(b1[47]));
    {
        let r = u!(rol8(w((tmp & 179).wrapping_add(68)), 2));
        b3[36] = w(((r & u!(b0[3])) | (tmp2 & !u!(b0[3]))).wrapping_sub(15));
    }
    b1[123] ^= 221;
    A = (i4!(b3[0]) / 3).wrapping_sub(i2!(b3[4]));
    C = (((u!(b3[0]) & 163).wrapping_add(92)) & 246) | (u!(b3[0]) & 92);
    E = ((C | u!(b3[24])) & 54) | (C & u!(b3[24]));
    b3[40] = w(A.wrapping_sub(E));
    b3[44] = w(tmp3 ^ 81 ^ (((u!(b3[0]) >> 1) & 101).wrapping_add(26)));
    b3[48] = w(i2!(b3[4]) & 27);
    b3[52] = 27;
    b3[56] = 199;
    {
        let (x40, x24) = (u!(b3[40]), u!(b3[24]));
        let y = i4!(b3[0]);
        let y20 = b4[(u!(b3[0]) % 20) as usize] as u32;
        let part1 = ((x40 | x24) & 177) | (x40 & x24);
        let part2 = ((y20 & 177) | 176) | (y & !3u32);
        let part3 = (((x40 & x24) | ((x40 | x24) & 177)) & 199)
            | ((((y & 1).wrapping_add(176)) | (y & !3u32)) & u!(b3[56]));
        b3[64] = w(u!(b3[4]).wrapping_add((((part1 & part2) | part3) & !u!(b3[52])) | u!(b3[48])));
    }
    b2[33] ^= b1[26];
    b1[106] ^= w(u!(b3[20]) ^ 133);
    b2[30] = w(((u!(b3[64]) / 3).wrapping_sub(275 | (u!(b3[0]) & 247))) ^ i0!(b1[122]));
    b1[22] = w((i2!(b1[90]) & 95) | 68);
    A = (i4!(b3[36]) & 184) | (i2!(b3[44]) & !184u32);
    b2[18] = w(u!(b2[18]).wrapping_add(A.wrapping_mul(A).wrapping_mul(A) >> 1));
    b2[5] = w(u!(b2[5]).wrapping_sub(i4!(b1[92])));
    A = (((u!(b1[41]) & !24u32) | (i2!(b1[183]) & 24)) & (u!(b3[16]).wrapping_add(53)))
        | (u!(b3[20]) & i2!(b3[20]));
    B = (u!(b1[17]) & !u!(b3[44])) | (i0!(b1[59]) & u!(b3[44]));
    b2[18] ^= w(A.wrapping_mul(B));
    A = weird_ror8(b1[11], i2!(b1[28]) & 7) & 7;
    B = (((i0!(b1[93]) & !u!(b0[14])) | (u!(b0[14]) & 150)) & !28u32) | (u!(b1[7]) & 28);
    {
        let r = weird_rol8(w(i2!(b3[0])), A);
        b2[22] = w((((B | r) & u!(b2[33])) | (B & r)).wrapping_add(74) & 0xff);
    }
    A = i4!(i0!(b1[39]) ^ 217);
    {
        let p = ((u!(b3[20]) | u!(b3[0])) & 214) | (u!(b3[20]) & u!(b3[0]));
        b0[15] = w(u!(b0[15]).wrapping_sub((p & A) | ((p | A) & u!(b3[32]))));
    }
    {
        let x = i2!(b1[57]);
        let y = i0!(b3[64]);
        B = ((x & y) | ((y | x) & 95) | (u!(b3[64]) & 45) | 82) & 32;
        C = ((x & y) | ((x | y) & 95)) & ((u!(b3[64]) & 45) | 82);
        D = ((u!(b3[0]) / 3).wrapping_sub(u!(b3[64]) | u!(b1[22])))
            ^ (u!(b3[28]).wrapping_add(62))
            ^ (B | C);
        T = b0[((D & 0xff) % 20) as usize] as u32;
    }
    {
        let x = i0!(b1[99]);
        b3[68] = w(x.wrapping_mul(x).wrapping_mul(x).wrapping_mul(x) | i2!(b3[64]));
    }
    U = i0!(b1[50]);
    W = i2!(b1[138]);
    X = i4!(b1[39]);
    Y = i0!(b1[4]);
    Z = i4!(b1[202]);
    V = i0!(b1[151]);
    S = i2!(b1[14]);
    R = i0!(b1[145]);

    A = (i2!(b3[68]) & i0!(b1[209])) | ((i2!(b3[68]) | i0!(b1[209])) & 24);
    B = weird_rol8(w(i4!(b1[127])), i2!(b3[68]) & 7);
    C = (A & u!(b0[10])) | (B & !u!(b0[10]));
    D = 7 ^ (i4!(i2!(b3[36])) << 1);
    b3[72] = w((C & 71) | (D & !71u32));
    {
        let left = ((i0!(b3[20]) << 1) & 159) | (i4!(b1[190]) & !159u32);
        let right = (((i4!(b3[64]) & 110) | (i0!(b1[25]) & !110u32)) & !150u32) | (u!(b1[25]) & 150);
        b2[2] = w(u!(b2[2]).wrapping_add(left & right));
    }
    b2[14] = w(u!(b2[14]).wrapping_sub(
        ((i2!(b3[20]) & (u!(b3[72]) ^ i2!(b1[100]))) & !34u32) | (u!(b1[97]) & 34),
    ));
    b0[17] = 115;
    {
        let p = i4!(b1[17]);
        let q = i0!(b3[20]);
        let r = ((p | q) & u!(b3[72])) | (p & q);
        let d3 = u!(b1[50]) / 3;
        b1[23] ^= w(((r & d3) | ((r | d3) & 246)) << 1);
    }
    {
        let p = i0!(b3[40]);
        b0[13] = w(((((p | u!(b1[10])) & 82) | (p & u!(b1[10]))) & 209 | ((i0!(b1[39]) << 1) & 46)) >> 1);
    }
    b2[33] = w(u!(b2[33]).wrapping_sub(u!(b1[113]) & 9));
    b2[28] = w(u!(b2[28]).wrapping_sub((((2 | (u!(b1[110]) & 222)) >> 1) & !223u32) | (u!(b3[20]) & 223)));
    J = weird_rol8(w(V | Z), U & 7);
    A = (u!(b2[16]) & T) | (W & !u!(b2[16]));
    B = (u!(b1[33]) & 17) | (X & !17u32);
    E = ((Y | (A.wrapping_add(B) / 5)) & 147) | (Y & (A.wrapping_add(B) / 5));
    {
        let idx = i4!((u!(b3[8]).wrapping_add(J).wrapping_add(E)) & 0xff);
        M = (u!(b3[40]) & idx) | ((u!(b3[40]) | idx) & u!(b2[23]));
    }
    {
        let x = i4!(b3[20]).wrapping_sub(48);
        b0[15] = w(((x & !u!(b1[184])) | (x & 189) | (189 & !u!(b1[184]))) & M.wrapping_mul(M).wrapping_mul(M));
    }
    b2[22] = w(u!(b2[22]).wrapping_add(u!(b1[183])));
    b3[76] = w(3u32.wrapping_mul(i4!(b1[1])) ^ u!(b3[0]));
    A = i2!((u!(b3[8]).wrapping_add(J.wrapping_add(E))) & 0xff);
    {
        let x = i4!(b1[178]);
        F = ((x & A) | ((x | A) & 209)).wrapping_mul(i0!(b1[13])).wrapping_mul(i4!(b1[26]) >> 1);
    }
    G = F
        .wrapping_add(0x733ffff9)
        .wrapping_mul(198)
        .wrapping_sub((F.wrapping_add(0x733ffff9).wrapping_mul(396).wrapping_add(212)) & 212)
        .wrapping_add(85);
    b3[80] = w(u!(b3[36])
        .wrapping_add(G ^ 148)
        .wrapping_add((G ^ 107) << 1)
        .wrapping_sub(127));
    b3[84] = w((i2!(b3[64]) & 245) | (i2!(b3[20]) & 10));
    A = i0!(b3[68]) | 81;
    b2[18] = w(u!(b2[18]).wrapping_sub(
        (A.wrapping_mul(A).wrapping_mul(A) & !u!(b0[15])) | ((u!(b3[80]) / 15) & u!(b0[15])),
    ));
    {
        let s = u!(b3[8]).wrapping_add(J).wrapping_add(E);
        b3[88] = w(s
            .wrapping_sub(i0!(b1[160]))
            .wrapping_add(i4!(i0!(s & 255)) / 3));
    }
    B = ((R ^ u!(b3[72])) & !198u32) | (S.wrapping_mul(S) & 198);
    {
        let x = i4!(b1[69]);
        F = (x & u!(b1[172])) | ((x | u!(b1[172])) & (u!(b3[12]).wrapping_sub(B).wrapping_add(77)));
    }
    b0[16] = w(147u32.wrapping_sub((u!(b3[72]) & ((F & 251) | 1)) | (((F & 250) | u!(b3[72])) & 198)));
    {
        let x = i4!(b1[168]);
        let y = i0!(b1[29]);
        C = (x & y & 7) | ((x | y) & 6);
    }
    {
        let x = i4!(b1[155]);
        F = (x & u!(b1[105])) | ((x | u!(b1[105])) & 141);
    }
    b0[3] = w(u!(b0[3]).wrapping_sub(i4!(weird_rol32(w(F), C))));
    b1[5] = w(weird_ror8(b0[12], (i0!(b1[61]) / 5) & 7) ^ ((!i2!(b3[84])) / 5));
    b1[198] = w(u!(b1[198]).wrapping_add(u!(b1[3])));
    A = 162 | i2!(b3[64]);
    b1[164] = w(u!(b1[164]).wrapping_add(A.wrapping_mul(A) / 5));
    G = weird_ror8(139, u!(b3[80]) & 7);
    {
        let x = i4!(b3[64]);
        C = (x.wrapping_mul(x).wrapping_mul(x) & 95) | (i0!(b3[40]) & !95u32);
    }
    b3[92] = w((G & 12) | (i0!(b3[20]) & 12) | (G & i0!(b3[20])) | C);
    b2[12] = w(u!(b2[12]).wrapping_add(((u!(b1[103]) & 32) | (u!(b3[92]) & (u!(b1[103]) | 60)) | 16) / 3));
    b3[96] = b1[143];
    b3[100] = 27;
    {
        let p = ((u!(b3[40]) & !u!(b2[8])) | (u!(b1[35]) & u!(b2[8]))) & u!(b3[64]);
        b3[104] = w(p ^ 119);
        b3[108] = w(238 & (p << 1));
        let q = !u!(b3[64]) & (u!(b3[84]) / 3);
        b3[112] = w(q ^ 49);
        b3[116] = w(98 & (q << 1));
    }
    A = (u!(b1[35]) & u!(b2[8])) | (u!(b3[40]) & !u!(b2[8]));
    B = (A & u!(b3[64])) | ((u!(b3[84]) / 3) & !u!(b3[64]));
    {
        let p = (((u!(b1[172]) & 65) >> 1) ^ 86)
            | ((!u!(b3[64]) & (u!(b3[84]) / 3))
                | (((u!(b3[40]) & !u!(b2[8])) | (u!(b1[35]) & u!(b2[8]))) & u!(b3[64])));
        b1[143] = w(u!(b3[96]).wrapping_sub((B & (86 + ((u!(b1[172]) & 64) >> 1))) | (p & u!(b3[100]))));
    }
    b2[29] = 162;
    A = ((i4!(b3[88]) & 160) | (i0!(b1[125]) & 95)) >> 1;
    B = i2!(b1[149]) ^ u!(b1[43]).wrapping_mul(u!(b1[43]));
    b0[15] = w(u!(b0[15]).wrapping_add((B & A) | ((A | B) & 115)));
    b3[120] = w(u!(b3[64]).wrapping_sub(i0!(b3[40])));
    b1[95] = w(i4!(b3[20]));
    {
        let x = i2!(b1[17]);
        A = weird_ror8(w(i2!(b3[80])), x.wrapping_mul(x).wrapping_mul(x) & 7);
    }
    b0[7] = w(u!(b0[7]).wrapping_sub(A.wrapping_mul(A)));
    {
        let x = i4!(b1[202]);
        b2[8] = w(u!(b2[8]).wrapping_sub(u!(b1[184])).wrapping_add(x.wrapping_mul(x).wrapping_mul(x)));
    }
    b0[16] = w((i2!(b1[102]) << 1) & 132);
    b3[124] = w((i4!(b3[40]) >> 1) ^ u!(b3[68]));
    b0[7] = w(u!(b0[7]).wrapping_sub(
        i0!(b1[191]).wrapping_sub(((i4!(b1[80]) << 1) & !177u32) | (i4!(i4!(b3[88])) & 177)),
    ));
    b0[6] = w(i0!(b1[119]));
    A = (i4!(b1[190]) & !209u32) | (u!(b1[118]) & 209);
    B = i0!(b3[120]).wrapping_mul(i0!(b3[120]));
    b0[12] = w((i0!(b3[84]) ^ i2!(b1[71]).wrapping_add(i2!(b1[15]))) & ((A & B) | ((A | B) & 27)));
    B = (u!(b1[32]) & i2!(b3[88])) | ((u!(b1[32]) | i2!(b3[88])) & 23);
    D = (i4!(b1[57]).wrapping_mul(231) & 169) | (B & 86);
    F = (((i0!(b1[82]) & !29u32) | (i4!(b3[124]) & 29)) & 190) | (i4!(D / 5) & !190u32);
    {
        let x = i0!(b3[40]);
        H = x.wrapping_mul(x).wrapping_mul(x);
    }
    K = (H & u!(b1[82])) | (H & 92) | (u!(b1[82]) & 92);
    b3[128] = w(((F & K) | ((F | K) & 192)) ^ (D / 5));
    b2[25] ^= w(
        ((i0!(b3[120]) << 1).wrapping_mul(u!(b1[5])))
            .wrapping_sub(weird_rol8(b3[76], i4!(b3[124]) & 7) & (u!(b3[20]).wrapping_add(110))),
    );
}

fn generate_session_key(old_sap: &[u8; 0x114], message_in: &[u8; 164], mode: usize) -> [u8; 16] {
    let decrypted = decrypt_message(message_in, mode);
    let mut new_sap = [0u8; 320];
    new_sap[0x000..0x011].copy_from_slice(&STATIC_SOURCE_1);
    new_sap[0x011..0x091].copy_from_slice(&decrypted);
    new_sap[0x091..0x111].copy_from_slice(&old_sap[0x80..0x100]);
    new_sap[0x111..0x140].copy_from_slice(&STATIC_SOURCE_2);
    let mut session_key = INITIAL_SESSION_KEY;
    for round in 0..5 {
        let base = &new_sap[round * 64..round * 64 + 64];
        let md5 = modified_md5(base, &session_key);
        sap_hash(base, &mut session_key);
        let mut words = le_words(&session_key);
        let md5_words = le_words(&md5);
        for i in 0..4 {
            words[i] = words[i].wrapping_add(md5_words[i]);
        }
        put_le_words(&mut session_key, words);
    }
    for i in (0..16).step_by(4) {
        session_key.swap(i, i + 3);
        session_key.swap(i + 1, i + 2);
    }
    for b in &mut session_key {
        *b ^= 121;
    }
    session_key
}

/// `playfair_decrypt`: entschlüsselt den 16-Byte-AES-Schlüssel aus `ekey` (72 Byte)
/// mit der 164-Byte-Nachricht aus fp-setup. Gibt `None` bei ungültigem Modus.
pub fn decrypt(message3: &[u8; 164], cipher_text: &[u8; 72]) -> Option<[u8; 16]> {
    // Sicherheitsprüfung: Das C-Original indiziert damit Tabellen ohne Prüfung.
    let mode = message3[12] as usize;
    if mode > 3 {
        return None;
    }
    let chunk1 = &cipher_text[16..32];
    let mut block_in: [u8; 16] = cipher_text[56..72].try_into().unwrap();
    let sap_key = generate_session_key(&DEFAULT_SAP, message3, mode);
    let schedule = generate_key_schedule(&sap_key);
    z_xor(&mut block_in);
    cycle(&mut block_in, &schedule);
    let mut key = [0u8; 16];
    for i in 0..16 {
        key[i] = block_in[i] ^ chunk1[i];
    }
    x_xor(&mut key);
    z_xor(&mut key);
    Some(key)
}

#[allow(dead_code)]
fn unused_sap_constants() -> ([u8; 16], [u8; 16]) {
    // Aus omg_hax.c; im Entschlüsselungspfad nicht benötigt (decrypt_sap), aber
    // für Vollständigkeit des Ports aufgeführt.
    (SAP_IV, SAP_KEY_MATERIAL)
}

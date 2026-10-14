//! Base64 (RFC 4648, Standardalphabet mit „=“) für die Netzdaten im Dokument (Spec §7, Plan
//! P12); selbst geschrieben, ohne neue Abhängigkeit. Das Lesen ist streng: keine Leerzeichen,
//! richtige Auffüllung, keine gesetzten Füllbits.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
/// Untere sechs Bit eines Zeichens.
const SIX_BITS: u32 = 0x3F;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Die Netzdaten sind keine gültige Base64-Zeichenkette")]
pub struct Base64Error;

fn symbol(n: u32) -> char {
    char::from(ALPHABET[(n & SIX_BITS) as usize])
}

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let (groups, rest) = bytes.as_chunks::<3>();
    for g in groups {
        let n = (u32::from(g[0]) << 16) | (u32::from(g[1]) << 8) | u32::from(g[2]);
        for shift in [18, 12, 6, 0] {
            out.push(symbol(n >> shift));
        }
    }
    match rest {
        [a] => {
            let n = u32::from(*a) << 16;
            out.push(symbol(n >> 18));
            out.push(symbol(n >> 12));
            out.push_str("==");
        }
        [a, b] => {
            let n = (u32::from(*a) << 16) | (u32::from(*b) << 8);
            out.push(symbol(n >> 18));
            out.push(symbol(n >> 12));
            out.push(symbol(n >> 6));
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Länge der dekodierten Bytes, ohne zu dekodieren.
pub fn decoded_len(text: &str) -> Result<usize, Base64Error> {
    let b = text.as_bytes();
    if !b.len().is_multiple_of(4) {
        return Err(Base64Error);
    }
    let pad = b.iter().rev().take(2).take_while(|&&c| c == b'=').count();
    Ok(b.len() / 4 * 3 - pad)
}

pub fn decode(text: &str) -> Result<Vec<u8>, Base64Error> {
    decode_bytes(text.as_bytes())
}

/// Dekodiert nur den Anfang: so viele Vierergruppen, wie für `max_bytes` Bytes nötig sind.
pub fn decode_prefix(text: &str, max_bytes: usize) -> Result<Vec<u8>, Base64Error> {
    let b = text.as_bytes();
    let end = (max_bytes.div_ceil(3) * 4).min(b.len());
    decode_bytes(&b[..end])
}

fn value(c: u8) -> Result<u32, Base64Error> {
    match c {
        b'A'..=b'Z' => Ok(u32::from(c - b'A')),
        b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(Base64Error),
    }
}

fn decode_bytes(b: &[u8]) -> Result<Vec<u8>, Base64Error> {
    if !b.len().is_multiple_of(4) {
        return Err(Base64Error);
    }
    let groups = b.as_chunks::<4>().0;
    let mut out = Vec::with_capacity(groups.len() * 3);
    for (k, g) in groups.iter().enumerate() {
        let pad = if k + 1 == groups.len() {
            g.iter().rev().take_while(|&&c| c == b'=').count()
        } else {
            0
        };
        if pad > 2 {
            return Err(Base64Error);
        }
        let mut n = 0u32;
        for &c in &g[..4 - pad] {
            n = (n << 6) | value(c)?;
        }
        n <<= 6 * pad as u32;
        // Füllbits müssen null sein (eindeutige Darstellung).
        let fill_mask = match pad {
            1 => 0xFF,
            2 => 0xFFFF,
            _ => 0,
        };
        if n & fill_mask != 0 {
            return Err(Base64Error);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Ok(out)
}

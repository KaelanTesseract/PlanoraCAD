//! Zeichensatz (WinAnsi) und Helvetica-Breiten für die PDF-Ausgabe.
//!
//! Quelle der Breiten: `Helvetica.afm` aus den "Core 14 AFM Files" von Adobe
//! (AFM 4.1, Version 002.000, Creation Date 1. Mai 1997). Die Werte wurden an zwei
//! unabhängigen Abschriften dieser Datei (Apache PDFBox, PDFKit) verglichen: identisch.
//! Die Breiten wurden nach den Glyphnamen der WinAnsi-Kodierung ausgelesen (nicht nach den
//! Codes der AFM-Datei, die StandardEncoding verwendet): 0x27 = `quotesingle`,
//! 0x60 = `grave`, 0x2D und 0xAD = `hyphen`, 0xA0 = `space`.
//! Belegung 0x80..=0x9F nach PDF-Referenz 1.4, Anhang D (WinAnsiEncoding).
//!
//! Copyright (c) 1985, 1987, 1989, 1990, 1997 Adobe Systems Incorporated.
//! All Rights Reserved. Helvetica is a trademark of Linotype-Hell AG and/or its subsidiaries.
//!
//! Nutzungsbedingungen der Core-14-AFM-Sammlung (Datei "MustRead.html", Adobe), sinngemäß:
//! Die AFM-Dateien dürfen für jeden Zweck, ohne Gebühr, mit oder ohne Änderung genutzt,
//! kopiert und weitergegeben werden, sofern alle Urheberrechtsvermerke erhalten bleiben, die
//! AFM-Dateien nicht ohne diesen Hinweis weitergegeben werden, Änderungen deutlich vermerkt
//! werden und der Hinweis selbst nicht verändert wird. Adobe übernimmt keine Pflicht zur
//! Unterstützung. Im Original (englisch): "This file and the 14 PostScript(R) AFM files it
//! accompanies may be used, copied, and distributed for any purpose and without charge, with or
//! without modification, provided that all copyright notices are retained; that the AFM files
//! are not distributed without this file; that all modifications to this file or any of the
//! AFM files are prominently noted in the modified file(s); and that this paragraph is not
//! modified. Adobe Systems has no responsibility or obligation to support the use of the AFM
//! files."
//! Hier ist nur die Breitentabelle (eine Teilmenge der Zahlen, neu angeordnet nach WinAnsi)
//! enthalten, nicht die AFM-Datei selbst; die Anordnung nach WinAnsi ist eine Änderung im
//! Sinne dieser Bedingungen.

/// Die 27 Zeichen auf 0x80..=0x9F (Byte, Zeichen).
const HIGH: [(u8, char); 27] = [
    (0x80, '\u{20AC}'),
    (0x82, '\u{201A}'),
    (0x83, '\u{0192}'),
    (0x84, '\u{201E}'),
    (0x85, '\u{2026}'),
    (0x86, '\u{2020}'),
    (0x87, '\u{2021}'),
    (0x88, '\u{02C6}'),
    (0x89, '\u{2030}'),
    (0x8A, '\u{0160}'),
    (0x8B, '\u{2039}'),
    (0x8C, '\u{0152}'),
    (0x8E, '\u{017D}'),
    (0x91, '\u{2018}'),
    (0x92, '\u{2019}'),
    (0x93, '\u{201C}'),
    (0x94, '\u{201D}'),
    (0x95, '\u{2022}'),
    (0x96, '\u{2013}'),
    (0x97, '\u{2014}'),
    (0x98, '\u{02DC}'),
    (0x99, '\u{2122}'),
    (0x9A, '\u{0161}'),
    (0x9B, '\u{203A}'),
    (0x9C, '\u{0153}'),
    (0x9E, '\u{017E}'),
    (0x9F, '\u{0178}'),
];

/// WinAnsi-Byte eines Zeichens. Das Minuszeichen U+2212 wird auf den Bindestrich 0x2D
/// abgebildet; alles, was WinAnsi nicht kennt, ergibt `None`.
pub fn winansi_byte(c: char) -> Option<u8> {
    let u = c as u32;
    match u {
        0x20..=0x7E => Some(u as u8),
        0x2212 => Some(0x2D),
        0xA0..=0xFF => Some(u as u8),
        _ => HIGH.iter().find(|&&(_, h)| h == c).map(|&(b, _)| b),
    }
}

/// Ob das Zeichen in der PDF-Ausgabe darstellbar ist.
pub fn pdf_char_ok(c: char) -> bool {
    winansi_byte(c).is_some()
}

/// Helvetica-Breiten in 1/1000 Geviert für die Bytes 0x20..=0xFF.
const WIDTHS: [u16; 224] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 0X20
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 0X30
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 0X40
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 0X50
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // 0X60
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 0, // 0X70
    556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, // 0X80
    0, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667, // 0X90
    278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333, // 0XA0
    400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611, // 0XB0
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278, // 0XC0
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611, // 0XD0
    556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278, // 0XE0
    556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500, // 0XF0
];

/// Breite eines WinAnsi-Bytes in 1/1000 Geviert; 0 für Bytes ohne Zeichen.
pub fn helvetica_width(byte: u8) -> u16 {
    if byte < 0x20 {
        0
    } else {
        WIDTHS[usize::from(byte - 0x20)]
    }
}

/// Textbreite bei der Schriftgröße `size` (Summe der Breiten · size / 1000); `None`,
/// sobald ein Zeichen nicht darstellbar ist.
pub fn pdf_text_width(text: &str, size: f64) -> Option<f64> {
    let mut sum: u32 = 0;
    for c in text.chars() {
        sum += u32::from(helvetica_width(winansi_byte(c)?));
    }
    Some(f64::from(sum) * size / 1000.0)
}

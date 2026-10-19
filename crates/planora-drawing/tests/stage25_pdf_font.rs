use planora_drawing::pdf_font::{helvetica_width, pdf_char_ok, pdf_text_width, winansi_byte};

fn w(c: char) -> u16 {
    helvetica_width(winansi_byte(c).unwrap())
}

#[test]
fn known_helvetica_widths() {
    let cases: &[(char, u16)] = &[
        (' ', 278),
        ('\'', 191),
        ('`', 333),
        ('"', 355),
        ('%', 889),
        ('&', 667),
        ('-', 333),
        ('.', 278),
        ('@', 1015),
        ('A', 667),
        ('M', 833),
        ('O', 778),
        ('W', 944),
        ('i', 222),
        ('l', 222),
        ('m', 833),
        ('w', 722),
        ('Ä', 667),
        ('Ö', 778),
        ('Ü', 722),
        ('ä', 556),
        ('ö', 556),
        ('ü', 556),
        ('ß', 611),
        ('Ø', 778),
        ('°', 400),
        ('±', 584),
        ('µ', 556),
        ('€', 556),
        ('…', 1000),
        ('–', 556),
        ('—', 1000),
        ('Œ', 1000),
        ('\u{A0}', 278),
        ('\u{AD}', 333),
    ];
    for &(c, width) in cases {
        assert_eq!(w(c), width, "Zeichen {c:?}");
    }
    for c in '0'..='9' {
        assert_eq!(w(c), 556);
    }
}

#[test]
fn winansi_mapping() {
    let pairs: &[(char, u8)] = &[
        ('ä', 0xE4),
        ('€', 0x80),
        ('…', 0x85),
        ('–', 0x96),
        ('—', 0x97),
        ('‚', 0x82),
        ('Ÿ', 0x9F),
        ('Œ', 0x8C),
        ('−', 0x2D),
        ('\u{A0}', 0xA0),
    ];
    for &(c, b) in pairs {
        assert_eq!(winansi_byte(c), Some(b), "Zeichen {c:?}");
    }
    for b in 0x20u8..=0x7E {
        assert_eq!(winansi_byte(b as char), Some(b));
    }
    for c in [
        '\n', '\t', '\u{7F}', '\u{80}', '\u{9F}', 'Ā', 'ő', '😀', '\u{2010}',
    ] {
        assert_eq!(winansi_byte(c), None, "Zeichen {c:?}");
    }
    let n = (0u32..=0xFFFF)
        .filter_map(char::from_u32)
        .filter(|&c| pdf_char_ok(c))
        .count();
    assert_eq!(n, 219);
}

#[test]
fn text_width() {
    let a = pdf_text_width("Helvetica", 10.0).unwrap();
    assert!((a - 41.12).abs() < 1e-9);
    let b = pdf_text_width("Ä Ø ß", 3.5).unwrap();
    assert!((b - (667.0 + 278.0 + 778.0 + 278.0 + 611.0) * 0.0035).abs() < 1e-12);
    let c = pdf_text_width("−5", 1.0).unwrap();
    assert!((c - 0.889).abs() < 1e-12);
    assert_eq!(pdf_text_width("a😀", 1.0), None);
    assert_eq!(pdf_text_width("", 5.0), Some(0.0));
}

#[test]
fn every_allowed_char_has_a_width() {
    let mut used = [false; 256];
    for c in (0u32..=0xFFFF).filter_map(char::from_u32) {
        if let Some(b) = winansi_byte(c) {
            assert!(helvetica_width(b) > 0, "Zeichen {c:?}");
            used[usize::from(b)] = true;
        }
    }
    for b in [0x81u8, 0x8D, 0x8F, 0x90, 0x9D] {
        assert!(!used[usize::from(b)]);
        assert_eq!(helvetica_width(b), 0);
    }
}

const REF: [u16; 224] = [
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

const BLOCK: [(u32, u8); 27] = [
    (0x20AC, 0x80),
    (0x201A, 0x82),
    (0x0192, 0x83),
    (0x201E, 0x84),
    (0x2026, 0x85),
    (0x2020, 0x86),
    (0x2021, 0x87),
    (0x02C6, 0x88),
    (0x2030, 0x89),
    (0x0160, 0x8A),
    (0x2039, 0x8B),
    (0x0152, 0x8C),
    (0x017D, 0x8E),
    (0x2018, 0x91),
    (0x2019, 0x92),
    (0x201C, 0x93),
    (0x201D, 0x94),
    (0x2022, 0x95),
    (0x2013, 0x96),
    (0x2014, 0x97),
    (0x02DC, 0x98),
    (0x2122, 0x99),
    (0x0161, 0x9A),
    (0x203A, 0x9B),
    (0x0153, 0x9C),
    (0x017E, 0x9E),
    (0x0178, 0x9F),
];

#[test]
fn all_224_widths_match_reference() {
    for byte in 0x20u8..=0xFF {
        assert_eq!(
            helvetica_width(byte),
            REF[usize::from(byte - 0x20)],
            "Byte {byte:#04X}"
        );
    }
    for byte in 0u8..0x20 {
        assert_eq!(helvetica_width(byte), 0);
    }
}

#[test]
fn block_80_9f_pairs_literal() {
    for (u, b) in BLOCK {
        let c = char::from_u32(u).unwrap();
        assert_eq!(winansi_byte(c), Some(b), "U+{u:04X}");
    }
    // Nachbarn der Lücken und Verwechslungskandidaten bleiben unzulässig.
    for u in [
        0x2031u32, 0x0191, 0x2010, 0x201B, 0x201F, 0x0081, 0x008D, 0x008F, 0x0090, 0x009D,
    ] {
        assert_eq!(winansi_byte(char::from_u32(u).unwrap()), None, "U+{u:04X}");
    }
}

#[test]
fn latin1_block_is_identity() {
    for u in 0xA0u32..=0xFF {
        let c = char::from_u32(u).unwrap();
        assert_eq!(winansi_byte(c), Some(u as u8), "U+{u:04X}");
    }
    assert_eq!(winansi_byte(char::from_u32(0xAD).unwrap()), Some(0xAD));
    assert_eq!(helvetica_width(0xAD), 333);
}

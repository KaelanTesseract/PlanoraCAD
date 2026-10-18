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

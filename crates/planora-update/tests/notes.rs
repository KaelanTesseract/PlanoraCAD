use planora_update::{NoteBlock, Span, parse_notes};

fn plain(text: &str) -> Span {
    Span {
        text: text.to_string(),
        bold: false,
    }
}

fn bold(text: &str) -> Span {
    Span {
        text: text.to_string(),
        bold: true,
    }
}

#[test]
fn headings_bullets_paragraphs_bold() {
    let md = "# Version 0.2.0\r\n\
              ## Neu\n\
              - Erste **wichtige** Sache\n\
              * Zweite Sache\n\
              \n\
              Ein Absatz\n\
              über zwei Zeilen.\n\
              \n\
              ### Behoben\n\
              **Alles fett**\n";
    let blocks = parse_notes(md);
    assert_eq!(
        blocks,
        [
            NoteBlock::Heading(1, vec![plain("Version 0.2.0")]),
            NoteBlock::Heading(2, vec![plain("Neu")]),
            NoteBlock::Bullet(vec![plain("Erste "), bold("wichtige"), plain(" Sache")]),
            NoteBlock::Bullet(vec![plain("Zweite Sache")]),
            NoteBlock::Paragraph(vec![plain("Ein Absatz über zwei Zeilen.")]),
            NoteBlock::Heading(3, vec![plain("Behoben")]),
            NoteBlock::Paragraph(vec![bold("Alles fett")]),
        ]
    );
}

#[test]
fn unknown_markdown_is_plain_text() {
    let blocks = parse_notes("#### Tief\n");
    assert_eq!(blocks, [NoteBlock::Paragraph(vec![plain("#### Tief")])]);

    let blocks = parse_notes("#ohne Leerzeichen");
    assert_eq!(
        blocks,
        [NoteBlock::Paragraph(vec![plain("#ohne Leerzeichen")])]
    );

    let blocks = parse_notes("[Link](https://example.com) und `Code` und _kursiv_");
    assert_eq!(
        blocks,
        [NoteBlock::Paragraph(vec![plain(
            "[Link](https://example.com) und `Code` und _kursiv_"
        )])]
    );

    // Ein nicht geschlossenes ** bleibt als Text stehen.
    let blocks = parse_notes("a **b** c **d");
    assert_eq!(
        blocks,
        [NoteBlock::Paragraph(vec![
            plain("a "),
            bold("b"),
            plain(" c **d")
        ])]
    );

    // Eingerückte Aufzählung wird flach dargestellt; „-ohne“ ist keine Aufzählung.
    let blocks = parse_notes("  - eingerückt\n-ohne");
    assert_eq!(
        blocks,
        [
            NoteBlock::Bullet(vec![plain("eingerückt")]),
            NoteBlock::Paragraph(vec![plain("-ohne")]),
        ]
    );

    // Leere fette Stelle erzeugt keinen leeren Abschnitt.
    let blocks = parse_notes("x****y");
    assert_eq!(blocks, [NoteBlock::Paragraph(vec![plain("xy")])]);
}

#[test]
fn empty_and_huge_input_never_panic() {
    assert!(parse_notes("").is_empty());
    assert!(parse_notes("\n\n  \r\n\t\n").is_empty());

    for odd in [
        "#",
        "# ",
        "-",
        "- ",
        "*",
        "* ",
        "**",
        "***",
        "****",
        "*****",
        "\r",
        "ä**ö",
        "# **",
        "- **x",
        "\u{feff}# Kopf",
        "\0",
        "##",
        "### ",
    ] {
        let _ = parse_notes(odd);
    }

    let huge = "- Punkt mit **fett** und Umlauten äöü\n".repeat(50_000)
        + &"Absatz ".repeat(100_000)
        + &"*".repeat(100_001);
    let blocks = parse_notes(&huge);
    assert!(blocks.len() >= 50_000);
}

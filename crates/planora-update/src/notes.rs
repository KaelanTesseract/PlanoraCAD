//! Kleiner Markdown-Leser für Patchnotes: Überschriften (`#`, `##`, `###`), Aufzählungen
//! (`-`, `*`), Absätze und `**fett**`. Alles andere bleibt normaler Text. Arbeitet zeilenweise
//! in linearer Zeit und gerät bei keiner Eingabe in Panik.

/// Ein Stück Text mit einheitlicher Auszeichnung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Der Text.
    pub text: String,
    /// Ob der Text fett dargestellt wird.
    pub bold: bool,
}

/// Ein Block der Patchnotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteBlock {
    /// Überschrift mit Ebene 1 bis 3.
    Heading(u8, Vec<Span>),
    /// Ein Aufzählungspunkt (verschachtelte Punkte werden flach dargestellt).
    Bullet(Vec<Span>),
    /// Ein Absatz; aufeinanderfolgende Zeilen werden mit Leerzeichen verbunden.
    Paragraph(Vec<Span>),
}

/// Zerlegt Patchnotes in Blöcke.
pub fn parse_notes(markdown: &str) -> Vec<NoteBlock> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();

    for line in markdown.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush_paragraph(&mut paragraph, &mut blocks);
            continue;
        }
        if let Some((level, text)) = heading(line) {
            flush_paragraph(&mut paragraph, &mut blocks);
            blocks.push(NoteBlock::Heading(level, spans(text)));
        } else if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            flush_paragraph(&mut paragraph, &mut blocks);
            blocks.push(NoteBlock::Bullet(spans(text.trim_start())));
        } else {
            paragraph.push(line);
        }
    }
    flush_paragraph(&mut paragraph, &mut blocks);
    blocks
}

fn heading(line: &str) -> Option<(u8, &str)> {
    [("### ", 3), ("## ", 2), ("# ", 1)]
        .into_iter()
        .find_map(|(prefix, level)| {
            line.strip_prefix(prefix)
                .map(|rest| (level, rest.trim_start()))
        })
}

fn flush_paragraph(lines: &mut Vec<&str>, blocks: &mut Vec<NoteBlock>) {
    if !lines.is_empty() {
        blocks.push(NoteBlock::Paragraph(spans(&lines.join(" "))));
        lines.clear();
    }
}

/// Zerlegt eine Zeile an `**` in normale und fette Stücke. Ein `**` ohne Gegenstück bleibt
/// als Text stehen; leere Stücke entfallen, gleich ausgezeichnete Nachbarn werden verbunden.
fn spans(text: &str) -> Vec<Span> {
    let mut parts: Vec<&str> = text.split("**").collect();
    // Bei ungerader Zahl von Markierungen ist die letzte ohne Gegenstück.
    let tail = if parts.len().is_multiple_of(2) {
        let last = parts.pop().unwrap_or_default();
        let before = parts.pop().unwrap_or_default();
        Some(format!("{before}**{last}"))
    } else {
        None
    };

    let mut out: Vec<Span> = Vec::new();
    let mut push = |text: &str, bold: bool| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(prev) if prev.bold == bold => prev.text.push_str(text),
            _ => out.push(Span {
                text: text.to_string(),
                bold,
            }),
        }
    };
    for (i, part) in parts.iter().enumerate() {
        push(part, i % 2 == 1);
    }
    if let Some(tail) = &tail {
        // Das Stück vor der offenen Markierung stand an gerader Stelle, ist also normaler Text.
        push(tail, false);
    }
    out
}

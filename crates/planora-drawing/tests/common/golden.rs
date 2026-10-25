//! Vergleich goldener Textausgaben (Debug-Text, SVG, DXF, PDF) über Betriebssysteme hinweg.
//!
//! `f64::sin`, `cos`, `atan2` usw. rufen die Mathematikbibliothek des Systems auf (Windows:
//! MSVC-Laufzeit, Linux: glibc). Deren Ergebnisse weichen gelegentlich in der letzten Binärstelle
//! voneinander ab; eine Zahl wird dann anders gerundet (`8.0` statt `7.999999999999999`,
//! `67.707442081` statt `67.70744208`). Die Fixturen sind unter Windows erzeugt. Der Vergleich
//! hier verlangt deshalb Gleichheit aller Zeichen außerhalb von Zahlen und bei Zahlen höchstens
//! anderthalb Einheiten der letzten ausgegebenen Stelle bzw. 1e-12 relativ, nie mehr.

/// Teil eines Textes: Zahl (Text, Wert, Nachkommastellen) oder anderes Zeichen.
#[derive(Debug)]
enum Tok<'a> {
    Num(&'a str, f64, i32, bool),
    Other(char),
}

fn tokens(s: &str) -> Vec<Tok<'_>> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let mut j = i;
        if b[j] == b'-' && j + 1 < b.len() && b[j + 1].is_ascii_digit() {
            j += 1;
        }
        // Eine Zahl beginnt nicht mitten in einem Wort (etwa `F1` oder `FEFF0047`).
        let in_word = start > 0 && (b[start - 1].is_ascii_alphanumeric() || b[start - 1] == b'_');
        if j < b.len() && b[j].is_ascii_digit() && !in_word {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let mut decimals = 0;
            let mut float = false;
            if j + 1 < b.len() && b[j] == b'.' && b[j + 1].is_ascii_digit() {
                float = true;
                j += 1;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                    decimals += 1;
                }
            }
            if j < b.len() && b[j] == b'e' {
                let mut k = j + 1;
                if k < b.len() && (b[k] == b'-' || b[k] == b'+') {
                    k += 1;
                }
                if k < b.len() && b[k].is_ascii_digit() {
                    while k < b.len() && b[k].is_ascii_digit() {
                        k += 1;
                    }
                    float = true;
                    j = k;
                }
            }
            // Folgt direkt ein Buchstabe, ist es ein Bezeichner (`3D`), keine Zahl.
            if j < b.len() && (b[j].is_ascii_alphabetic() || b[j] == b'_') {
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                out.extend(s[start..j].chars().map(Tok::Other));
            } else {
                let text = &s[start..j];
                out.push(Tok::Num(text, text.parse().unwrap(), decimals, float));
            }
            i = j;
        } else {
            let c = s[i..].chars().next().unwrap();
            out.push(Tok::Other(c));
            i += c.len_utf8();
        }
    }
    out
}

/// Gleich bis auf die Rundung der letzten Stelle von Kommazahlen? `Err` nennt die erste
/// Abweichung mit Umgebung.
pub fn same_up_to_last_digit(stored: &str, written: &str) -> Result<(), String> {
    if stored == written {
        return Ok(());
    }
    let (a, b) = (tokens(stored), tokens(written));
    let around = |k: usize, t: &[Tok]| -> String {
        t[k.saturating_sub(40)..(k + 40).min(t.len())]
            .iter()
            .map(|t| match t {
                Tok::Num(s, ..) => s.to_string(),
                Tok::Other(c) => c.to_string(),
            })
            .collect()
    };
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        let ok = match (x, y) {
            (Tok::Other(c), Tok::Other(d)) => c == d,
            (Tok::Num(s, ..), Tok::Num(t, ..)) if s == t => true,
            (Tok::Num(_, u, du, fu), Tok::Num(_, v, dv, fv)) if *fu || *fv => {
                let last = 1.5 * 10f64.powi(-(*du.max(dv)));
                let rel = 1e-12 * u.abs().max(v.abs()).max(1.0);
                (u - v).abs() <= last.max(rel)
            }
            _ => false,
        };
        if !ok {
            return Err(format!(
                "Abweichung bei Teil {k}:\n  Fixtur:    …{}…\n  Schreiber: …{}…",
                around(k, &a),
                around(k, &b)
            ));
        }
    }
    if a.len() != b.len() {
        return Err(format!(
            "verschieden lang: Fixtur {} Teile, Schreiber {} Teile",
            a.len(),
            b.len()
        ));
    }
    Ok(())
}

/// PDF ohne die Byte-Versätze (`/Length`, Querverweistabelle, `startxref`): Sie folgen aus den
/// Längen der Zahlen davor. Ob sie stimmen, prüft der strenge Leser (`pdf_read::read`).
pub fn without_pdf_offsets(pdf: &str) -> String {
    let mut out = String::new();
    let mut after_startxref = false;
    for line in pdf.split_inclusive('\n') {
        let body = line.trim_end_matches('\n').trim_end();
        let xref_entry = body.len() == 18
            && body.as_bytes()[..10].iter().all(u8::is_ascii_digit)
            && body.as_bytes()[11..16].iter().all(u8::is_ascii_digit)
            && matches!(&body[16..], " n" | " f");
        if after_startxref || xref_entry {
            out += "#\n";
        } else {
            out += &mask_length(line);
        }
        after_startxref = body == "startxref";
    }
    out
}

fn mask_length(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find("/Length ") {
        out += &rest[..i + 8];
        rest = rest[i + 8..].trim_start_matches(|c: char| c.is_ascii_digit());
        out += "#";
    }
    out + rest
}

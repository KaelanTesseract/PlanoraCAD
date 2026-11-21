//! Lizenzen von Plug-ins: welche SPDX-Ausdrücke zu PlanoraCAD (GPL-3.0) passen. Die Liste ist
//! genau die `allow`-Liste aus `deny.toml` (ein Test hält beide gleich); der Installationsdialog
//! zeigt bei einer anderen Lizenz nur einen Hinweis, er verbietet nichts.

/// Die GPL-verträglichen Lizenzen (wie `[licenses] allow` in `deny.toml`).
pub const GPL_COMPATIBLE: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "ISC",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "Zlib",
    "Unicode-3.0",
    "0BSD",
    "CC0-1.0",
    "BSL-1.0",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
];

/// Ob der SPDX-Ausdruck `spdx` GPL-verträglich ist: bei `OR` genügt eine verträgliche Seite,
/// bei `AND` müssen alle verträglich sein (`AND` bindet stärker, Klammern gehen vor). `A WITH B`
/// gilt nur, wenn genau diese Form in [`GPL_COMPATIBLE`] steht. Kennungen zählen ohne Rücksicht
/// auf Groß- und Kleinschreibung. Ein leerer oder nicht lesbarer Ausdruck ist nicht verträglich.
pub fn license_ok(spdx: &str) -> bool {
    let spaced = spdx.replace('(', " ( ").replace(')', " ) ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut pos = 0;
    match parse_or(&tokens, &mut pos) {
        Some(ok) => pos == tokens.len() && ok,
        None => false,
    }
}

/// `und ("OR" und)*`; `None` bei einem Fehler im Ausdruck.
fn parse_or(tokens: &[&str], pos: &mut usize) -> Option<bool> {
    let mut ok = parse_and(tokens, pos)?;
    while tokens.get(*pos) == Some(&"OR") {
        *pos += 1;
        ok |= parse_and(tokens, pos)?;
    }
    Some(ok)
}

/// `glied ("AND" glied)*`.
fn parse_and(tokens: &[&str], pos: &mut usize) -> Option<bool> {
    let mut ok = parse_term(tokens, pos)?;
    while tokens.get(*pos) == Some(&"AND") {
        *pos += 1;
        ok &= parse_term(tokens, pos)?;
    }
    Some(ok)
}

/// `"(" oder ")"` oder `Lizenz ["WITH" Ausnahme]`.
fn parse_term(tokens: &[&str], pos: &mut usize) -> Option<bool> {
    let token = *tokens.get(*pos)?;
    *pos += 1;
    if token == "(" {
        let ok = parse_or(tokens, pos)?;
        if tokens.get(*pos) != Some(&")") {
            return None;
        }
        *pos += 1;
        return Some(ok);
    }
    if matches!(token, ")" | "AND" | "OR" | "WITH") {
        return None;
    }
    if tokens.get(*pos) == Some(&"WITH") {
        let exception = *tokens.get(*pos + 1)?;
        if matches!(exception, "(" | ")" | "AND" | "OR" | "WITH") {
            return None;
        }
        *pos += 2;
        let full = format!("{token} WITH {exception}");
        return Some(compatible(&full));
    }
    Some(compatible(token))
}

/// Steht `id` in [`GPL_COMPATIBLE`] (SPDX-Kennungen ohne Rücksicht auf Groß- und Kleinschreibung)?
fn compatible(id: &str) -> bool {
    GPL_COMPATIBLE.iter().any(|l| l.eq_ignore_ascii_case(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn license_list_matches_deny_toml() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../deny.toml");
        let text = std::fs::read_to_string(path).unwrap();
        let table: toml::Table = toml::from_str(&text).unwrap();
        let allow: Vec<&str> = table["licenses"]["allow"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(allow, GPL_COMPATIBLE);
    }

    #[test]
    fn license_expressions() {
        assert!(license_ok("MIT OR Proprietary"));
        assert!(!license_ok("MIT AND Proprietary"));
        assert!(license_ok("Apache-2.0 WITH LLVM-exception"));
        assert!(!license_ok("GPL-2.0-only"));
        // Weitere Formen: Klammern, Vorrang (AND vor OR), WITH nur wie in der Liste.
        assert!(license_ok("MIT AND (Apache-2.0 OR Proprietary)"));
        assert!(license_ok("Proprietary OR MIT AND ISC"));
        assert!(!license_ok("(Proprietary OR GPL-2.0-only) AND MIT"));
        assert!(!license_ok("MIT WITH LLVM-exception"));
        assert!(license_ok("  GPL-3.0-only "));
        assert!(!license_ok(""));
        assert!(!license_ok("MIT OR"));
        assert!(!license_ok("(MIT"));
        assert!(!license_ok("MIT ISC"));
        // SPDX-Kennungen ohne Rücksicht auf Groß- und Kleinschreibung; GPL-3.0-or-later passt.
        assert!(license_ok("mit"));
        assert!(license_ok("gpl-3.0-ONLY"));
        assert!(license_ok("GPL-3.0-or-later"));
        assert!(license_ok("apache-2.0 WITH llvm-exception"));
    }
}

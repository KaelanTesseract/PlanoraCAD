//! Verweise auf Parameter im Inhalt eines Skizzentextes (Skizzen 11b, Spec 4): `{Name}` wird
//! durch den Wert des Parameters ersetzt. Hier nur das Erkennen; das Ersetzen kommt mit der
//! Rechnung (Task 11).

use crate::expr::check_name;

/// Enthält `content` einen Verweis `{Name}` mit einem Namen, den [`check_name`] annimmt? Ohne
/// Leerzeichen in der Klammer; `{}` oder `{1a}` sind kein Verweis.
pub fn has_param_ref(content: &str) -> bool {
    let mut rest = content;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(['{', '}']) else {
            return false;
        };
        if after[close..].starts_with('}') && check_name(&after[..close]).is_ok() {
            return true;
        }
        rest = &after[close..];
    }
    false
}

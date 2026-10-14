//! Zeichenketten im STEP-Format (Spec §4): in Hochkommas; `'` wird `''`, `\` wird `\\`; Zeichen
//! außerhalb von ASCII als `\X2\…\X0\` (vier Hexziffern je Zeichen) bzw. `\X4\…\X0\` (acht
//! Hexziffern, Zeichen jenseits von U+FFFF); Steuerzeichen werden Leerzeichen.

use std::fmt::Write as _;

/// Höchstzahl der Zeichen eines Namens (Spec §4); längere Namen werden gekürzt.
pub const MAX_NAME_CHARS: usize = 256;
/// Größtes Zeichen, das in einen `\X2\`-Abschnitt passt (vier Hexziffern).
const X2_MAX: u32 = 0xFFFF;

/// Fertige STEP-Zeichenkette samt Hinweis, ob gekürzt wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepText {
    /// Mit Hochkommas, nur druckbares ASCII.
    pub text: String,
    pub shortened: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Run {
    Ascii,
    X2,
    X4,
}

/// Wandelt `name` in eine STEP-Zeichenkette (höchstens [`MAX_NAME_CHARS`] Zeichen).
pub fn step_string(name: &str) -> StepText {
    let shortened = name.chars().count() > MAX_NAME_CHARS;
    let mut out = String::from("'");
    let mut run = Run::Ascii;
    for c in name.chars().take(MAX_NAME_CHARS) {
        let c = if c.is_control() { ' ' } else { c };
        let want = if c.is_ascii() {
            Run::Ascii
        } else if u32::from(c) <= X2_MAX {
            Run::X2
        } else {
            Run::X4
        };
        if want != run {
            if run != Run::Ascii {
                out.push_str("\\X0\\");
            }
            match want {
                Run::X2 => out.push_str("\\X2\\"),
                Run::X4 => out.push_str("\\X4\\"),
                Run::Ascii => {}
            }
            run = want;
        }
        match want {
            Run::Ascii => match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            },
            Run::X2 => {
                let _ = write!(out, "{:04X}", u32::from(c));
            }
            Run::X4 => {
                let _ = write!(out, "{:08X}", u32::from(c));
            }
        }
    }
    if run != Run::Ascii {
        out.push_str("\\X0\\");
    }
    out.push('\'');
    StepText {
        text: out,
        shortened,
    }
}

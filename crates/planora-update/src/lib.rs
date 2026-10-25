//! Update-Funktion, reine Logik ohne Netz und Oberfläche: Versionsnummern und Kanäle,
//! die Release-Liste von GitHub lesen, das passende Update wählen und Patchnotes zerlegen.

mod channel;
mod notes;
mod release;
mod version;

pub use channel::Channel;
pub use notes::{NoteBlock, Span, parse_notes};
pub use release::{
    AssetRef, EXE_NAME, LICENSES_NAME, MAX_NOTES_CHARS, MAX_NOTES_RELEASES, REPO, Release,
    ReleaseError, SHA256_NAME, Update, parse_releases, pick_update,
};
pub use version::{ParseError, Pre, PreKind, Version};

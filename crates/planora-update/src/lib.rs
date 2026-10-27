//! Update-Funktion ohne Oberfläche: Versionsnummern und Kanäle,
//! die Release-Liste von GitHub lesen, das passende Update wählen und Patchnotes zerlegen.
//! Dazu die Prüfung auf neue Versionen ([`check`]) hinter einer austauschbaren Quelle; nur
//! [`check::HttpSource`] geht ins Netz. Download und Prüfung des Installers stehen in
//! [`install`]; dort geht nur [`install::HttpDownloader`] ins Netz. Installiert wird vom
//! Installer selbst, nicht von dieser Bibliothek.

mod channel;
pub mod check;
pub mod install;
mod net;
mod notes;
mod release;
pub mod sha256;
mod version;

pub use channel::Channel;
pub use notes::{NoteBlock, Span, parse_notes};
pub use release::{
    AssetRef, LICENSES_NAME, MAX_NOTES_CHARS, MAX_NOTES_RELEASES, REPO, Release, ReleaseError,
    SETUP_NAME, SHA256_NAME, Update, parse_releases, pick_update,
};
pub use version::{ParseError, Pre, PreKind, Version};

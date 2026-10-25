//! Update-Kanäle: Dev, Beta, Stabil.

use crate::version::Version;

/// Update-Kanal des Nutzers. Ein Kanal schließt die stabileren mit ein.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Alle Versionen (Dev, Beta, Stabil).
    Dev,
    /// Beta- und stabile Versionen.
    Beta,
    /// Nur Versionen ohne Endung.
    Stable,
}

impl Channel {
    /// Alle Kanäle in der Reihenfolge der Auswahlliste.
    pub const ALL: [Channel; 3] = [Channel::Dev, Channel::Beta, Channel::Stable];

    /// Kennung für die Konfiguration: `"dev"`, `"beta"`, `"stable"`.
    pub fn id(self) -> &'static str {
        match self {
            Channel::Dev => "dev",
            Channel::Beta => "beta",
            Channel::Stable => "stable",
        }
    }

    /// Anzeigename in der Oberfläche.
    pub fn label(self) -> &'static str {
        match self {
            Channel::Dev => "Dev",
            Channel::Beta => "Beta",
            Channel::Stable => "Stabil",
        }
    }

    /// Kanal zur Kennung aus [`Channel::id`]; unbekannte Kennungen ergeben `None`
    /// (genaue Schreibweise, kein Kürzen von Leerzeichen).
    pub fn from_id(id: &str) -> Option<Channel> {
        Channel::ALL.into_iter().find(|c| c.id() == id)
    }

    /// Ob eine Version zu diesem Kanal zählt: Dev alle, Beta Beta und Stabil, Stabil nur
    /// Versionen ohne Endung.
    pub fn includes(self, version: &Version) -> bool {
        match self {
            Channel::Dev => true,
            Channel::Beta => version.channel() != Channel::Dev,
            Channel::Stable => version.channel() == Channel::Stable,
        }
    }
}

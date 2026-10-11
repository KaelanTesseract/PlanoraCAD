#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("Dateifehler bei „{path}“: {msg}")]
    Io { path: String, msg: String },
    #[error("Die Datei „{path}“ ist beschädigt: {msg}")]
    Corrupt { path: String, msg: String },
    #[error(
        "Die Datei „{path}“ wurde mit einer neueren Programmversion erstellt (Format {found})."
    )]
    TooNew { path: String, found: u32 },
    #[error("{0}")]
    InvalidName(String),
    #[error("Der Pfad liegt außerhalb des Projektordners.")]
    OutsideRoot,
    #[error("„{0}“ gibt es schon.")]
    Exists(String),
    #[error("„{0}“ wurde nicht gefunden.")]
    NotFound(String),
    #[error("Das Dokument wird gerade von einem anderen Programm benutzt (gesperrt seit {since}).")]
    Locked { since: String },
    #[error(
        "Die Sperre wurde von einem anderen Programm übernommen. Das Dokument wurde nicht verändert; bitte neu öffnen."
    )]
    LockLost,
    #[error("{0}")]
    Invalid(String),
    #[error(
        "Diese Version ist nicht die neueste ihres Zweigs. Bitte einen neuen Zweignamen angeben."
    )]
    NeedsBranch,
}

impl ProjectError {
    pub fn io(path: &std::path::Path, e: &std::io::Error) -> Self {
        ProjectError::Io {
            path: path.display().to_string(),
            msg: e.to_string(),
        }
    }

    pub fn corrupt(path: impl Into<String>, msg: impl Into<String>) -> Self {
        ProjectError::Corrupt {
            path: path.into(),
            msg: msg.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, ProjectError>;

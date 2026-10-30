use planora_i18n::tr;
use std::fmt;

/// Fehler der Projektdateien. Die Texte entstehen beim Anzeigen in der aktuellen Sprache; die
/// Texte in `InvalidName`, `Invalid` und `msg` sind schon in der Sprache fertig, in der der Fehler
/// entstand.
#[derive(Debug)]
pub enum ProjectError {
    Io { path: String, msg: String },
    Corrupt { path: String, msg: String },
    TooNew { path: String, found: u32 },
    InvalidName(String),
    OutsideRoot,
    Exists(String),
    NotFound(String),
    Locked { since: String },
    LockLost,
    Invalid(String),
    NeedsBranch,
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ProjectError::Io { path, msg } => {
                tr!("proj-err-io", path = path.as_str(), msg = msg.as_str())
            }
            ProjectError::Corrupt { path, msg } => {
                tr!("proj-err-corrupt", path = path.as_str(), msg = msg.as_str())
            }
            ProjectError::TooNew { path, found } => {
                tr!(
                    "proj-err-too-new",
                    path = path.as_str(),
                    found = found.to_string()
                )
            }
            ProjectError::InvalidName(text) | ProjectError::Invalid(text) => text.clone(),
            ProjectError::OutsideRoot => tr!("proj-err-outside-root"),
            ProjectError::Exists(name) => tr!("proj-err-exists", name = name.as_str()),
            ProjectError::NotFound(name) => tr!("proj-err-not-found", name = name.as_str()),
            ProjectError::Locked { since } => tr!("proj-err-locked", since = since.as_str()),
            ProjectError::LockLost => tr!("proj-err-lock-lost"),
            ProjectError::NeedsBranch => tr!("proj-err-needs-branch"),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for ProjectError {}

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

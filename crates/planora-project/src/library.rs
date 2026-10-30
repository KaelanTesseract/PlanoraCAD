//! Projektbaum auf der Festplatte: Projekte → Ordner → Dokumente.

use crate::error::{ProjectError, Result};
use crate::fsutil::{Root, TRASH, atomic_write, new_id, validate_name};
use planora_i18n::tr;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "projekt.json";
pub const DOCUMENT_FILE: &str = "dokument.json";
pub const DOCUMENT_SUFFIX: &str = ".pcad";
/// Projektdateien dieser Version tragen diese Formatnummer.
const PROJECT_FORMAT: u32 = 1;

pub type NodePath = Vec<String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Project,
    Folder,
    Document,
}

#[derive(Clone, Debug)]
pub struct TreeNode {
    /// Anzeigename (bei Dokumenten ohne `.pcad`).
    pub name: String,
    pub kind: NodeKind,
    pub path: NodePath,
    pub children: Vec<TreeNode>,
}

pub struct Library {
    root: Root,
}

fn display_name(part: &str) -> String {
    part.strip_suffix(DOCUMENT_SUFFIX)
        .unwrap_or(part)
        .to_string()
}

/// Ist der Eintrag ein echter Ordner (kein Link, kein Junction-Verweis)? Links werden im Baum
/// nie verfolgt, damit keine Schleifen und keine Ziele außerhalb der Wurzel entstehen.
fn is_real_dir(e: &fs::DirEntry) -> bool {
    e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()) && !is_reparse_point(e)
}

#[cfg(windows)]
fn is_reparse_point(e: &fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    /// Windows-Attribut für Links und Junctions.
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    e.metadata()
        .map(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(true)
}

#[cfg(not(windows))]
fn is_reparse_point(_e: &fs::DirEntry) -> bool {
    false
}

impl Library {
    pub fn open(root: &Path) -> Result<Library> {
        Ok(Library {
            root: Root::open(root)?,
        })
    }

    pub fn root(&self) -> &Root {
        &self.root
    }

    pub fn document_dir(&self, path: &NodePath) -> Result<PathBuf> {
        let dir = self.root.resolve(path)?;
        if path.last().is_some_and(|p| p.ends_with(DOCUMENT_SUFFIX))
            && dir.join(DOCUMENT_FILE).is_file()
        {
            Ok(dir)
        } else {
            Err(ProjectError::NotFound(path.join("/")))
        }
    }

    fn kind_of(&self, path: &NodePath) -> Result<NodeKind> {
        let dir = self.root.resolve(path)?;
        if !dir.is_dir() {
            return Err(ProjectError::NotFound(path.join("/")));
        }
        if path.last().is_some_and(|p| p.ends_with(DOCUMENT_SUFFIX))
            && dir.join(DOCUMENT_FILE).is_file()
        {
            Ok(NodeKind::Document)
        } else if path.len() == 1 && dir.join(PROJECT_FILE).is_file() {
            Ok(NodeKind::Project)
        } else if path.len() > 1 && !path.last().is_some_and(|p| p.ends_with(DOCUMENT_SUFFIX)) {
            Ok(NodeKind::Folder)
        } else {
            Err(ProjectError::Invalid(tr!("proj-not-a-node")))
        }
    }

    /// Ort für Ordner und Dokumente: ein Projekt oder ein Ordner.
    fn require_container(&self, path: &NodePath) -> Result<()> {
        match self.kind_of(path)? {
            NodeKind::Document => Err(ProjectError::Invalid(tr!("proj-target-is-document"))),
            _ => Ok(()),
        }
    }

    pub fn tree(&self) -> Result<Vec<TreeNode>> {
        let mut out = Vec::new();
        let rd =
            fs::read_dir(self.root.path()).map_err(|e| ProjectError::io(self.root.path(), &e))?;
        for e in rd {
            let e = e.map_err(|e| ProjectError::io(self.root.path(), &e))?;
            let name = e.file_name().to_string_lossy().to_string();
            if name == TRASH
                || name.ends_with(DOCUMENT_SUFFIX)
                || !is_real_dir(&e)
                || !e.path().join(PROJECT_FILE).is_file()
            {
                continue;
            }
            let path = vec![name];
            // Fremde Ordner mit unzulässigem Namen gehören nicht in den Baum.
            if self.root.resolve(&path).is_err() {
                continue;
            }
            out.push(self.node(path, NodeKind::Project)?);
        }
        sort_nodes(&mut out);
        Ok(out)
    }

    fn node(&self, path: NodePath, kind: NodeKind) -> Result<TreeNode> {
        let mut children = Vec::new();
        if kind != NodeKind::Document {
            let dir = self.root.resolve(&path)?;
            for e in fs::read_dir(&dir).map_err(|e| ProjectError::io(&dir, &e))? {
                let e = e.map_err(|e| ProjectError::io(&dir, &e))?;
                if !is_real_dir(&e) {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let mut p = path.clone();
                p.push(name.clone());
                if self.root.resolve(&p).is_err() {
                    continue; // Name, den PlanoraCAD nicht anlegen würde
                }
                let k = if name.ends_with(DOCUMENT_SUFFIX) {
                    if e.path().join(DOCUMENT_FILE).is_file() {
                        NodeKind::Document
                    } else {
                        continue; // beschädigter Dokumentordner ohne dokument.json
                    }
                } else {
                    NodeKind::Folder
                };
                children.push(self.node(p, k)?);
            }
            sort_nodes(&mut children);
        }
        let name = display_name(path.last().map(String::as_str).unwrap_or_default());
        Ok(TreeNode {
            name,
            kind,
            path,
            children,
        })
    }

    pub fn create_project(&self, name: &str) -> Result<NodePath> {
        let name = validate_name(name)?;
        if name.ends_with(DOCUMENT_SUFFIX) {
            return Err(ProjectError::InvalidName(tr!("proj-project-name-suffix")));
        }
        let path = vec![name.clone()];
        let dir = self.root.resolve(&path)?;
        if fs::symlink_metadata(&dir).is_ok() {
            return Err(ProjectError::Exists(name));
        }
        fs::create_dir(&dir).map_err(|e| ProjectError::io(&dir, &e))?;
        let text = format!(
            "{{\n  \"format\": {PROJECT_FORMAT},\n  \"id\": \"{}\",\n  \"name\": {}\n}}\n",
            new_id(),
            json_string(&name)
        );
        if let Err(e) = atomic_write(&dir.join(PROJECT_FILE), text.as_bytes()) {
            // Kein halb angelegtes Projekt zurücklassen.
            let _ = fs::remove_dir(&dir);
            return Err(e);
        }
        Ok(path)
    }

    pub fn create_folder(&self, parent: &NodePath, name: &str) -> Result<NodePath> {
        if self.kind_of(parent)? == NodeKind::Document {
            return Err(ProjectError::Invalid(tr!("proj-no-folder-in-document")));
        }
        let name = validate_name(name)?;
        if name.ends_with(DOCUMENT_SUFFIX) {
            return Err(ProjectError::InvalidName(tr!("proj-folder-name-suffix")));
        }
        let mut path = parent.clone();
        path.push(name.clone());
        let dir = self.root.resolve(&path)?;
        if fs::symlink_metadata(&dir).is_ok() {
            return Err(ProjectError::Exists(name));
        }
        fs::create_dir(&dir).map_err(|e| ProjectError::io(&dir, &e))?;
        Ok(path)
    }

    pub fn rename(&self, path: &NodePath, new_name: &str) -> Result<NodePath> {
        let kind = self.kind_of(path)?;
        let name = validate_name(new_name)?;
        if kind != NodeKind::Document && name.ends_with(DOCUMENT_SUFFIX) {
            return Err(ProjectError::InvalidName(tr!("proj-name-not-allowed")));
        }
        let part = if kind == NodeKind::Document {
            format!("{name}{DOCUMENT_SUFFIX}")
        } else {
            name
        };
        let mut to = path.clone();
        if let Some(last) = to.last_mut() {
            *last = part;
        }
        if &to == path {
            return Ok(to);
        }
        self.rename_dir(path, &to)?;
        Ok(to)
    }

    pub fn move_node(&self, path: &NodePath, new_parent: &NodePath) -> Result<NodePath> {
        let kind = self.kind_of(path)?;
        if kind == NodeKind::Project {
            return Err(ProjectError::Invalid(tr!("proj-no-move-project")));
        }
        self.require_container(new_parent)?;
        if new_parent.starts_with(path) {
            return Err(ProjectError::Invalid(tr!("proj-no-move-into-self")));
        }
        let mut to = new_parent.clone();
        to.push(path.last().cloned().unwrap_or_default());
        if &to == path {
            return Ok(to);
        }
        self.rename_dir(path, &to)?;
        Ok(to)
    }

    fn rename_dir(&self, from: &NodePath, to: &NodePath) -> Result<()> {
        let a = self.root.resolve(from)?;
        let b = self.root.resolve(to)?;
        if fs::symlink_metadata(&b).is_ok() {
            // Nur Groß-/Kleinschreibung ändern ist erlaubt (dasselbe Verzeichnis).
            let same =
                matches!((fs::canonicalize(&a), fs::canonicalize(&b)), (Ok(x), Ok(y)) if x == y);
            if !same {
                return Err(ProjectError::Exists(display_name(
                    to.last().map(String::as_str).unwrap_or_default(),
                )));
            }
        }
        fs::rename(&a, &b).map_err(|e| ProjectError::io(&b, &e))
    }

    /// Ist `path` ein Projekt oder Ordner (also ein Ort für Ordner und Dokumente)?
    pub fn is_container(&self, path: &NodePath) -> bool {
        matches!(self.kind_of(path), Ok(NodeKind::Project | NodeKind::Folder))
    }

    pub fn delete(&self, path: &NodePath) -> Result<()> {
        if path.is_empty() {
            return Err(ProjectError::Invalid(tr!("proj-nothing-selected")));
        }
        self.kind_of(path)?;
        self.root.to_trash(path)
    }
}

fn sort_nodes(v: &mut [TreeNode]) {
    v.sort_by_key(|n| n.name.to_lowercase());
}

/// Text als JSON-Zeichenkette (nur für die kleine Projektdatei).
fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

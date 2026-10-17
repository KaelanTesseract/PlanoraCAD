//! Blattangaben des Schriftfelds: Eingabefelder mit Vorbelegung („automatisch“), Änderungen mit
//! Buchstaben, Grenzen und die Auflösung zu den Texten, die auf das Blatt kommen.

use crate::model::{Drawing, Scale};
use crate::sheet_frame::{REV_VISIBLE, TitleField};
use std::collections::BTreeMap;

pub const MAX_FIELD_CHARS: usize = 80;
pub const MAX_TITLE_CHARS: usize = 60;
pub const MAX_NOTE_CHARS: usize = 100;
pub const MAX_REVISION_CHARS: usize = 120;
pub const MAX_NOTES: usize = 6;
pub const MAX_REVISIONS: usize = 23;
pub const TOO_MANY_REVISIONS: &str = "Es sind höchstens 23 Änderungen möglich.";
pub const DEFAULT_DOC_TYPE: &str = "Fertigungszeichnung";
pub const DEFAULT_STATUS: &str = "in Arbeit";
pub const DASH: &str = "–";
pub const DEFAULT_TOLERANCE: &str = "ISO 2768-m";
pub const LANGUAGE: &str = "de";

/// Ein Eintrag: automatisch oder eigener Text.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum FieldValue {
    #[default]
    Auto,
    Text(String),
}

impl FieldValue {
    /// Der eigene Text, sonst der automatische Wert.
    pub fn resolve<'a>(&'a self, auto: &'a str) -> &'a str {
        match self {
            FieldValue::Auto => auto,
            FieldValue::Text(t) => t,
        }
    }

    /// Leere Eingabe oder nur Leerraum bedeutet automatisch.
    pub fn from_input(s: &str) -> FieldValue {
        if s.trim().is_empty() {
            FieldValue::Auto
        } else {
            FieldValue::Text(s.to_string())
        }
    }
}

/// Hinweiszeilen: automatisch oder eigene Zeilen (auch keine).
#[derive(Clone, Debug, PartialEq, Default)]
pub enum NotesValue {
    #[default]
    Auto,
    Lines(Vec<String>),
}

/// Angaben für das ganze Projekt.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectInfo {
    pub login: String,
    pub owner: FieldValue,
    pub creator: FieldValue,
    pub checked: FieldValue,
    pub approved: FieldValue,
    pub material: FieldValue,
    pub tolerance: FieldValue,
    pub notes: NotesValue,
}

/// Eine Änderung in der Änderungstabelle.
#[derive(Clone, Debug, PartialEq)]
pub struct Revision {
    pub letter: FieldValue,
    pub description: String,
    pub date: String,
    pub name: String,
}

/// Angaben eines Zeichnungsblatts.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetInfo {
    pub created: String,
    pub number: FieldValue,
    pub title: FieldValue,
    pub subtitle: FieldValue,
    pub doc_type: FieldValue,
    pub status: FieldValue,
    pub issued: FieldValue,
    pub revision: FieldValue,
    pub sheet_of: FieldValue,
    pub format: FieldValue,
    pub scale: FieldValue,
    pub weight: FieldValue,
    pub notes: NotesValue,
    pub revisions: Vec<Revision>,
}

/// Datum und Benutzer beim Anlegen.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamp {
    pub date: String,
    pub user: String,
}

impl Stamp {
    /// Heutiges Datum (UTC) und angemeldeter Benutzer (leer, wenn unbekannt).
    pub fn now() -> Stamp {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let user = ["USERNAME", "USER"]
            .iter()
            .find_map(|k| {
                std::env::var(k)
                    .ok()
                    .map(|v| clean_user(&v))
                    .filter(|v| !v.is_empty())
            })
            .unwrap_or_default();
        Stamp {
            date: utc_date(secs),
            user,
        }
    }
}

/// Macht einen Benutzernamen für das Schriftfeld brauchbar: ohne Steuerzeichen, ohne Leerraum an
/// den Enden und höchstens [`MAX_FIELD_CHARS`] Zeichen lang.
pub fn clean_user(raw: &str) -> String {
    let no_ctrl: String = raw.chars().filter(|c| !c.is_control()).collect();
    no_ctrl.trim().chars().take(MAX_FIELD_CHARS).collect()
}

/// Datum „JJJJ-MM-TT“ (UTC) zu Sekunden seit 1970.
pub fn utc_date(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    // Bürgerliches Datum aus Tagen seit 1970-01-01 (Algorithmus von Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Buchstabe der Änderung `index` (ab 0): A bis Z ohne I, O und Q; ab 23 keiner.
pub fn revision_letter(index: usize) -> Option<char> {
    ('A'..='Z')
        .filter(|c| !matches!(c, 'I' | 'O' | 'Q'))
        .nth(index)
}

impl ProjectInfo {
    pub fn new(stamp: &Stamp) -> ProjectInfo {
        ProjectInfo {
            login: stamp.user.clone(),
            owner: FieldValue::Auto,
            creator: FieldValue::Auto,
            checked: FieldValue::Auto,
            approved: FieldValue::Auto,
            material: FieldValue::Auto,
            tolerance: FieldValue::Auto,
            notes: NotesValue::Auto,
        }
    }
}

impl SheetInfo {
    pub fn new(stamp: &Stamp) -> SheetInfo {
        SheetInfo {
            created: stamp.date.clone(),
            number: FieldValue::Auto,
            title: FieldValue::Auto,
            subtitle: FieldValue::Auto,
            doc_type: FieldValue::Auto,
            status: FieldValue::Auto,
            issued: FieldValue::Auto,
            revision: FieldValue::Auto,
            sheet_of: FieldValue::Auto,
            format: FieldValue::Auto,
            scale: FieldValue::Auto,
            weight: FieldValue::Auto,
            notes: NotesValue::Auto,
            revisions: Vec::new(),
        }
    }

    /// Hängt eine Änderung an (Buchstabe automatisch); ab der 24. Fehler.
    pub fn add_revision(&mut self, date: &str, name: &str) -> Result<(), &'static str> {
        if self.revisions.len() >= MAX_REVISIONS {
            return Err(TOO_MANY_REVISIONS);
        }
        self.revisions.push(Revision {
            letter: FieldValue::Auto,
            description: String::new(),
            date: date.to_string(),
            name: name.to_string(),
        });
        Ok(())
    }
}

/// Die vorbelegten Hinweiszeilen.
pub fn default_notes(tolerance: &str) -> Vec<String> {
    vec![
        format!("Allgemeintoleranzen {tolerance}"),
        "Kanten gebrochen 0,3 mm".to_string(),
        "Oberflächen Ra 3,2 (soweit nicht anders angegeben)".to_string(),
    ]
}

fn check_text(what: &str, s: &str, max: usize) -> Result<(), String> {
    if s.chars().any(char::is_control) {
        return Err(format!("{what}: Steuerzeichen sind nicht erlaubt."));
    }
    if s.chars().count() > max {
        return Err(format!("{what} ist zu lang (höchstens {max} Zeichen)."));
    }
    Ok(())
}

fn check_value(what: &str, v: &FieldValue, max: usize) -> Result<(), String> {
    match v {
        FieldValue::Auto => Ok(()),
        FieldValue::Text(t) => check_text(what, t, max),
    }
}

fn check_notes(n: &NotesValue) -> Result<(), String> {
    if let NotesValue::Lines(l) = n {
        if l.len() > MAX_NOTES {
            return Err(format!(
                "Es sind höchstens {MAX_NOTES} Hinweiszeilen möglich."
            ));
        }
        for line in l {
            check_text("Ein Hinweis", line, MAX_NOTE_CHARS)?;
        }
    }
    Ok(())
}

/// Prüft Längen (in Zeichen), Zeilenzahl, Zahl der Änderungen und Steuerzeichen.
pub fn validate_sheet_info(i: &SheetInfo) -> Result<(), String> {
    check_text("Das Erstellungsdatum", &i.created, MAX_FIELD_CHARS)?;
    check_value("Die Zeichnungsnummer", &i.number, MAX_FIELD_CHARS)?;
    check_value("Der Titel", &i.title, MAX_TITLE_CHARS)?;
    check_value("Der Zusatztitel", &i.subtitle, MAX_FIELD_CHARS)?;
    check_value("Die Dokumentart", &i.doc_type, MAX_FIELD_CHARS)?;
    check_value("Der Dokumentstatus", &i.status, MAX_FIELD_CHARS)?;
    check_value("Das Ausgabedatum", &i.issued, MAX_FIELD_CHARS)?;
    check_value("Der Änderungsstand", &i.revision, MAX_FIELD_CHARS)?;
    check_value("Die Blattangabe", &i.sheet_of, MAX_FIELD_CHARS)?;
    check_value("Das Format", &i.format, MAX_FIELD_CHARS)?;
    check_value("Der Maßstab", &i.scale, MAX_FIELD_CHARS)?;
    check_value("Das Gewicht", &i.weight, MAX_FIELD_CHARS)?;
    check_notes(&i.notes)?;
    if i.revisions.len() > MAX_REVISIONS {
        return Err(TOO_MANY_REVISIONS.to_string());
    }
    for r in &i.revisions {
        check_value("Der Änderungsbuchstabe", &r.letter, MAX_FIELD_CHARS)?;
        check_text(
            "Die Beschreibung der Änderung",
            &r.description,
            MAX_REVISION_CHARS,
        )?;
        check_text("Das Datum der Änderung", &r.date, MAX_FIELD_CHARS)?;
        check_text("Der Name bei der Änderung", &r.name, MAX_FIELD_CHARS)?;
    }
    Ok(())
}

pub fn validate_project_info(p: &ProjectInfo) -> Result<(), String> {
    check_text("Der Benutzername", &p.login, MAX_FIELD_CHARS)?;
    check_value("Der Eigentümer", &p.owner, MAX_FIELD_CHARS)?;
    check_value("Der Ersteller", &p.creator, MAX_FIELD_CHARS)?;
    check_value("Der Prüfer", &p.checked, MAX_FIELD_CHARS)?;
    check_value("Die Freigabe", &p.approved, MAX_FIELD_CHARS)?;
    check_value("Der Werkstoff", &p.material, MAX_FIELD_CHARS)?;
    check_value("Die Allgemeintoleranz", &p.tolerance, MAX_FIELD_CHARS)?;
    check_notes(&p.notes)
}

/// Stellung des Blatts im Dokument.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetContext {
    pub doc_name: String,
    /// Ab 1.
    pub index: usize,
    pub count: usize,
}

/// Die aufgelösten Texte des Schriftfelds.
#[derive(Clone, Debug, PartialEq)]
pub struct TitleTexts {
    pub fields: BTreeMap<TitleField, String>,
    pub revisions: Vec<[String; 4]>,
    pub notes: Vec<String>,
}

impl TitleTexts {
    pub fn get(&self, f: TitleField) -> &str {
        self.fields.get(&f).map(String::as_str).unwrap_or("")
    }

    /// Die letzten drei Änderungen (`REV_VISIBLE`), neueste zuletzt.
    pub fn visible_revisions(&self) -> &[[String; 4]] {
        &self.revisions[self.revisions.len().saturating_sub(REV_VISIBLE)..]
    }
}

/// Maßstab für das Schriftfeld: der der ersten Ansicht der Zeichnung (Grundansicht, Reihenfolge
/// der Zeichnung); ohne Ansichten `None` (im Feld ein Strich). Die Zahl der Linien zählt nicht:
/// sonst gewönne oft die isometrische Ansicht, die einen eigenen Maßstab haben kann.
pub fn main_scale(d: &Drawing) -> Option<Scale> {
    d.views.first().map(|v| v.scale)
}

/// Löst Blattangaben, Projektangaben und Vorgaben zu den Texten des Schriftfelds auf.
pub fn resolve_texts(
    d: &Drawing,
    project: Option<&ProjectInfo>,
    ctx: &SheetContext,
    main: Option<Scale>,
) -> TitleTexts {
    let info = d.info.as_ref();
    let pick = |v: Option<&FieldValue>, auto: &str| -> String {
        v.map_or(auto, |v| v.resolve(auto)).to_string()
    };
    let auto_number = format!("{}-{:02}", ctx.doc_name, ctx.index);
    let auto_sheet = format!("{} von {}", ctx.index, ctx.count);
    let auto_scale = main.map_or_else(|| DASH.to_string(), |s| s.label());
    let issued_auto = info.map_or(DASH, |i| i.created.as_str());
    // Projektfelder: ohne Projekt ein Strich.
    let proj = |f: fn(&ProjectInfo) -> &FieldValue, auto: &str| -> String {
        project.map_or_else(|| DASH.to_string(), |p| f(p).resolve(auto).to_string())
    };

    let mut fields = BTreeMap::new();
    fields.insert(TitleField::Owner, proj(|p| &p.owner, ""));
    fields.insert(
        TitleField::Number,
        pick(info.map(|i| &i.number), &auto_number),
    );
    fields.insert(TitleField::Title, pick(info.map(|i| &i.title), &d.name));
    fields.insert(TitleField::Subtitle, pick(info.map(|i| &i.subtitle), ""));
    fields.insert(
        TitleField::DocType,
        pick(info.map(|i| &i.doc_type), DEFAULT_DOC_TYPE),
    );
    fields.insert(
        TitleField::Status,
        pick(info.map(|i| &i.status), DEFAULT_STATUS),
    );
    fields.insert(
        TitleField::Issued,
        pick(info.map(|i| &i.issued), issued_auto),
    );

    let rows: Vec<[String; 4]> = info
        .map(|i| {
            i.revisions
                .iter()
                .enumerate()
                .map(|(k, r)| {
                    let auto = revision_letter(k).map(String::from).unwrap_or_default();
                    [
                        r.letter.resolve(&auto).to_string(),
                        r.description.clone(),
                        r.date.clone(),
                        r.name.clone(),
                    ]
                })
                .collect()
        })
        .unwrap_or_default();
    let last_letter = rows.last().map(|r| r[0].clone()).unwrap_or_default();
    fields.insert(
        TitleField::Revision,
        pick(info.map(|i| &i.revision), &last_letter),
    );
    fields.insert(
        TitleField::SheetOf,
        pick(info.map(|i| &i.sheet_of), &auto_sheet),
    );
    fields.insert(
        TitleField::Format,
        pick(info.map(|i| &i.format), d.sheet.paper.label()),
    );
    fields.insert(TitleField::Scale, pick(info.map(|i| &i.scale), &auto_scale));
    let login = project.map_or("", |p| p.login.as_str());
    fields.insert(TitleField::Creator, proj(|p| &p.creator, login));
    fields.insert(TitleField::Checked, proj(|p| &p.checked, ""));
    fields.insert(TitleField::Approved, proj(|p| &p.approved, ""));
    fields.insert(TitleField::Material, proj(|p| &p.material, DASH));
    fields.insert(TitleField::Weight, pick(info.map(|i| &i.weight), DASH));
    fields.insert(TitleField::Language, LANGUAGE.to_string());

    let tolerance = project.map_or(DEFAULT_TOLERANCE, |p| {
        p.tolerance.resolve(DEFAULT_TOLERANCE)
    });
    let project_notes = match project.map(|p| &p.notes) {
        Some(NotesValue::Lines(l)) => l.clone(),
        _ => default_notes(tolerance),
    };
    let notes = match info.map(|i| &i.notes) {
        Some(NotesValue::Lines(l)) => l.clone(),
        _ => project_notes,
    };

    TitleTexts {
        fields,
        revisions: rows,
        notes,
    }
}

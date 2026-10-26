//! Issue-Formulare unter `.github/ISSUE_TEMPLATE`: feste Feld-Ids, Feldtypen, Pflichtfelder, Labels,
//! Bereichsliste und `config.yml`. Die Dateien werden ohne YAML-Bibliothek zeilenweise gelesen;
//! die Formulare halten dafür eine feste Einrückung ein (`  - type:`, `    id:`,
//! `    validations:`, `      required: true`, `        - Option`). Ob die Dateien gültiges YAML
//! und ein gültiges GitHub-Formularschema sind, prüft dieser Test nicht; das macht der PyYAML-Lauf
//! (Plan, Task 1, Schritt 4).

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Ein Feld des Formulars (Block ab `  - type:`).
struct Field {
    kind: String,
    id: Option<String>,
    required: bool,
    options: Vec<String>,
}

fn fields(text: &str) -> Vec<Field> {
    let mut out: Vec<Field> = Vec::new();
    let mut in_options = false;
    let mut in_validations = false;
    for line in text.lines() {
        if let Some(kind) = line.strip_prefix("  - type: ") {
            out.push(Field {
                kind: kind.trim().to_string(),
                id: None,
                required: false,
                options: Vec::new(),
            });
            in_options = false;
            in_validations = false;
            continue;
        }
        let Some(cur) = out.last_mut() else { continue };
        if let Some(id) = line.strip_prefix("    id: ") {
            cur.id = Some(id.trim().to_string());
        } else if line == "    validations:" {
            in_validations = true;
            in_options = false;
        } else if line == "      options:" {
            in_options = true;
            in_validations = false;
        } else if line == "      required: true" && in_validations {
            cur.required = true;
        } else if let Some(opt) = line.strip_prefix("        - ") {
            if in_options {
                cur.options.push(opt.trim().to_string());
            }
        } else if !line.starts_with("        ") && !line.starts_with("      ") {
            // andere Zeile auf höherer Ebene beendet die Listen
            in_options = in_options && line.trim().is_empty();
        }
    }
    out
}

fn ids(text: &str) -> Vec<String> {
    fields(text).into_iter().filter_map(|f| f.id).collect()
}

fn required(text: &str) -> Vec<String> {
    fields(text)
        .into_iter()
        .filter(|f| f.required)
        .filter_map(|f| f.id)
        .collect()
}

const AREAS: [&str; 6] = [
    "Skizze",
    "3D",
    "Export",
    "Zeichnung",
    "Tastenkürzel",
    "Anderes",
];

#[test]
fn bug_form_has_fixed_ids_and_required_fields() {
    let t = read(".github/ISSUE_TEMPLATE/fehler.yml");
    assert_eq!(
        ids(&t),
        [
            "ziel",
            "passiert",
            "erwartet",
            "bereich",
            "version",
            "system",
            "grafik",
            "schritte",
            "protokoll",
            "bild"
        ]
    );
    assert_eq!(required(&t), ["ziel", "passiert", "erwartet", "bereich"]);
    assert!(t.lines().any(|l| l == "labels: [\"bug\"]"));
    assert!(t.lines().any(|l| l == "title: \"Fehler: \""));
}

#[test]
fn idea_form_has_fixed_ids_and_required_fields() {
    let t = read(".github/ISSUE_TEMPLATE/wunsch.yml");
    assert_eq!(
        ids(&t),
        ["wunsch", "heute", "wichtig", "bereich", "angaben"]
    );
    assert_eq!(required(&t), ["wunsch", "heute", "bereich"]);
    assert!(t.lines().any(|l| l == "labels: [\"idee\"]"));
    assert!(t.lines().any(|l| l == "title: \"Wunsch: \""));
    assert!(t.contains('👍'));
    let wichtig = fields(&t)
        .into_iter()
        .find(|f| f.id.as_deref() == Some("wichtig"))
        .unwrap();
    assert_eq!(
        wichtig.options,
        ["Wäre schön", "Wichtig", "Ohne geht es kaum"]
    );
}

#[test]
fn area_options_are_the_six_areas() {
    for file in ["fehler.yml", "wunsch.yml"] {
        let t = read(&format!(".github/ISSUE_TEMPLATE/{file}"));
        let bereich = fields(&t)
            .into_iter()
            .find(|f| f.id.as_deref() == Some("bereich"))
            .unwrap();
        assert_eq!(bereich.kind, "dropdown", "{file}");
        assert_eq!(bereich.options, AREAS, "{file}");
    }
}

#[test]
fn config_disables_blank_issues_and_links_both_pages() {
    let t = read(".github/ISSUE_TEMPLATE/config.yml");
    assert!(t.lines().any(|l| l == "blank_issues_enabled: false"));
    let base = "https://github.com/KaelanTesseract/PlanoraCAD/blob/master/";
    for page in ["docs/haeufige-probleme.md", "docs/so-meldest-du-etwas.md"] {
        let url = format!("{base}{page}");
        assert!(t.contains(&format!("url: {url}")), "{url} fehlt");
        assert!(repo_root().join(page).is_file(), "{page} fehlt im Repo");
    }
    assert!(t.contains("name: Häufige Probleme"));
    assert!(t.contains("name: So meldest du etwas"));
}

#[test]
fn ids_are_url_safe() {
    for file in ["fehler.yml", "wunsch.yml"] {
        let t = read(&format!(".github/ISSUE_TEMPLATE/{file}"));
        for id in ids(&t) {
            assert!(
                !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{file}: {id}"
            );
        }
    }
}

#[test]
fn app_field_ids_exist_in_the_forms() {
    use planora_app::feedback::*;
    let fehler = ids(&read(".github/ISSUE_TEMPLATE/fehler.yml"));
    for id in [
        FIELD_PASSIERT,
        FIELD_VERSION,
        FIELD_SYSTEM,
        FIELD_GRAFIK,
        FIELD_SCHRITTE,
        FIELD_PROTOKOLL,
    ] {
        assert!(fehler.iter().any(|f| f == id), "fehler.yml: {id}");
    }
    let wunsch = ids(&read(".github/ISSUE_TEMPLATE/wunsch.yml"));
    for id in [FIELD_WUNSCH, FIELD_ANGABEN] {
        assert!(wunsch.iter().any(|f| f == id), "wunsch.yml: {id}");
    }
    for kind in [Kind::Fehler, Kind::Wunsch, Kind::Frage] {
        let file = format!(".github/ISSUE_TEMPLATE/{}", kind.template());
        let t = read(&file);
        let ids = ids(&t);
        assert!(ids.iter().any(|f| f == kind.description_field()), "{file}");
        if kind != Kind::Frage {
            assert!(
                t.lines()
                    .any(|l| l == format!("title: \"{}\"", kind.title_prefix())),
                "{file}"
            );
        }
    }
    assert_eq!(planora_app::feedback::AREAS, self::AREAS);
}

#[test]
fn field_types_are_as_designed() {
    let kinds = |file: &str, id: &str| {
        let t = read(&format!(".github/ISSUE_TEMPLATE/{file}"));
        fields(&t)
            .into_iter()
            .find(|f| f.id.as_deref() == Some(id))
            .unwrap_or_else(|| panic!("{file}: {id}"))
            .kind
    };
    for id in ["ziel", "passiert", "erwartet"] {
        assert_eq!(kinds("fehler.yml", id), "textarea", "{id}");
    }
    assert_eq!(kinds("wunsch.yml", "wichtig"), "dropdown");
}

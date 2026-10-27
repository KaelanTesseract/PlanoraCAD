//! Rückmeldung ohne Oberfläche: persönliche Angaben entfernen, die Adresse für das
//! GitHub-Formular bauen (höchstens [`MAX_URL_LEN`] Zeichen, bei Bedarf gekürzt) und den
//! Rückmeldungsordner mit `rueckmeldung.txt` anlegen. Das Fenster dazu liegt in
//! `feedback_dialog.rs`.

use crate::build_info::BuildInfo;
use std::path::{Path, PathBuf};

/// Feld „Was ist passiert?“ im Fehler-Formular.
pub const FIELD_PASSIERT: &str = "passiert";
/// Feld „Was möchtest du tun können?“ im Wunsch-Formular.
pub const FIELD_WUNSCH: &str = "wunsch";
pub const FIELD_VERSION: &str = "version";
pub const FIELD_SYSTEM: &str = "system";
pub const FIELD_GRAFIK: &str = "grafik";
pub const FIELD_SCHRITTE: &str = "schritte";
pub const FIELD_PROTOKOLL: &str = "protokoll";
/// Beim Wunsch (und bei der Frage) gehen alle Angaben gesammelt in dieses Feld.
pub const FIELD_ANGABEN: &str = "angaben";

/// Die Bereiche wie in der Auswahl „Bereich“ der Formulare.
pub const AREAS: [&str; 6] = [
    "Skizze",
    "3D",
    "Export",
    "Zeichnung",
    "Tastenkürzel",
    "Anderes",
];

/// Webformular auf dennis-guse.de: der Weg ohne GitHub-Konto (deutsch).
pub const WEBFORM_URL: &str = "https://dennis-guse.de/planoracad/rueckmeldung.php";
/// Dasselbe Formular auf Englisch (gleiche Felder); vorgehalten für eine spätere englische
/// Oberfläche, derzeit ungenutzt.
pub const WEBFORM_URL_EN: &str = "https://dennis-guse.de/en/planoracad/feedback.php";

/// Die Adresse bleibt nach dem Kodieren kürzer als dieser Wert.
pub const MAX_URL_LEN: usize = 2000;
/// Endet eine gekürzte Beschreibung in der Adresse.
pub const KUERZUNG_HINWEIS: &str = "… (gekürzt, ganz in der angehängten Datei)";
/// Steht am Ende des Beschreibungsfelds, wenn es eine Datei zum Anhängen gibt.
pub const ATTACH_HINT: &str = "Datei hier anhängen: rueckmeldung.txt";
/// Wie [`ATTACH_HINT`], wenn auch `bild.png` im Ordner liegt (Bestandteil „Bild vom Fenster“).
pub const ATTACH_HINT_PICTURE: &str = "Dateien hier anhängen: rueckmeldung.txt und bild.png";
/// Höchstlänge der Treiberangabe der Grafikkarte.
pub const MAX_DRIVER_CHARS: usize = 120;

/// Unterordner des Datenordners für Rückmeldungen.
pub const FEEDBACK_DIR: &str = "rueckmeldungen";
/// Name der Datei mit dem vollen Text.
pub const FEEDBACK_FILE: &str = "rueckmeldung.txt";

/// Seite „Häufige Probleme“ im Repo (Menü „Hilfe“, Formular-Auswahl auf GitHub).
pub fn faq_url() -> String {
    doc_url("docs/haeufige-probleme.md")
}

/// Anleitung „So meldest du etwas“ im Repo.
pub fn guide_url() -> String {
    doc_url("docs/so-meldest-du-etwas.md")
}

/// Adresse einer Datei im Repo auf dem Standardzweig `master`.
fn doc_url(path: &str) -> String {
    format!(
        "https://github.com/{}/blob/master/{path}",
        planora_update::REPO
    )
}

const USER_MARK: &str = "<Benutzer>";
const MAIL_MARK: &str = "<Mailadresse>";

/// Art der Rückmeldung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fehler,
    Wunsch,
    Frage,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Fehler => "Fehler",
            Kind::Wunsch => "Wunsch",
            Kind::Frage => "Frage",
        }
    }

    /// Formular unter `.github/ISSUE_TEMPLATE`. Fragen nutzen das Wunsch-Formular.
    pub fn template(self) -> &'static str {
        match self {
            Kind::Fehler => "fehler.yml",
            Kind::Wunsch | Kind::Frage => "wunsch.yml",
        }
    }

    pub fn title_prefix(self) -> &'static str {
        match self {
            Kind::Fehler => "Fehler: ",
            Kind::Wunsch => "Wunsch: ",
            Kind::Frage => "Frage: ",
        }
    }

    /// Feld, in das die Beschreibung der Testerin geht.
    pub fn description_field(self) -> &'static str {
        match self {
            Kind::Fehler => FIELD_PASSIERT,
            Kind::Wunsch | Kind::Frage => FIELD_WUNSCH,
        }
    }
}

/// Ein Bestandteil der Rückmeldung mit eigenem Häkchen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Version,
    System,
    Grafik,
    Schritte,
    Protokoll,
    Bild,
    Projektdatei,
    Absturzbericht,
}

impl Part {
    pub fn label(self) -> &'static str {
        match self {
            Part::Version => "Version",
            Part::System => "System",
            Part::Grafik => "Grafikkarte",
            Part::Schritte => "Letzte Schritte",
            Part::Protokoll => "Protokollzeilen",
            Part::Bild => "Bild vom Fenster",
            Part::Projektdatei => "Projektdatei",
            Part::Absturzbericht => "Absturzbericht",
        }
    }

    /// Ob das Häkchen am Anfang gesetzt ist: nur bei Version, System, Grafikkarte und letzten
    /// Schritten. Den Absturzbericht hakt das Fenster nur an, wenn es aus der Absturz-Rückfrage
    /// geöffnet wurde (`feedback_dialog::FeedbackContext::from_crash`).
    pub fn default_on(self) -> bool {
        matches!(
            self,
            Part::Version | Part::System | Part::Grafik | Part::Schritte
        )
    }

    /// Feld im Fehler-Formular; `None` für Bestandteile, die nur als Datei mitgehen.
    fn bug_field(self) -> Option<&'static str> {
        match self {
            Part::Version => Some(FIELD_VERSION),
            Part::System => Some(FIELD_SYSTEM),
            Part::Grafik => Some(FIELD_GRAFIK),
            Part::Schritte => Some(FIELD_SCHRITTE),
            Part::Protokoll | Part::Absturzbericht => Some(FIELD_PROTOKOLL),
            Part::Bild | Part::Projektdatei => None,
        }
    }
}

/// Was aus Pfaden und Texten verschwinden soll.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Personal {
    pub user_names: Vec<String>,
    pub home_dirs: Vec<String>,
    pub project_paths: Vec<PathBuf>,
}

impl Personal {
    /// Aus `USERNAME`, `USER`, `USERPROFILE` und `HOME`; der letzte Teil des Benutzerordners
    /// zählt ebenfalls als Name.
    pub fn from_env(project_paths: Vec<PathBuf>) -> Personal {
        let var = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let mut home_dirs: Vec<String> = Vec::new();
        for k in ["USERPROFILE", "HOME"] {
            if let Some(h) = var(k) {
                let h = h.trim_end_matches(['\\', '/']).to_string();
                if h.chars().count() > 3 && !home_dirs.contains(&h) {
                    home_dirs.push(h);
                }
            }
        }
        let mut user_names: Vec<String> = Vec::new();
        let mut add = |n: &str| {
            if n.chars().count() >= MIN_NAME_CHARS && !user_names.iter().any(|u| u == n) {
                user_names.push(n.to_string());
            }
        };
        for k in ["USERNAME", "USER"] {
            if let Some(n) = var(k) {
                add(&n);
            }
        }
        for h in &home_dirs {
            if let Some(last) = h.rsplit(['\\', '/']).find(|s| !s.is_empty()) {
                add(last);
            }
        }
        Personal {
            user_names,
            home_dirs,
            project_paths,
        }
    }
}

/// Kürzere Namen werden nicht im Freitext ersetzt (zu viele Treffer); Pfade unter
/// `Users`/`home` deckt die allgemeine Regel trotzdem ab.
const MIN_NAME_CHARS: usize = 3;

fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// Gleich ohne Groß/Klein; `\` und `/` gelten als gleich.
fn same_char(a: char, b: char) -> bool {
    (is_sep(a) && is_sep(b)) || a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Treffer von `needle` in `hay` (beides als Zeichen), ohne Groß/Klein. Liefert Start und Ende.
fn find_ci(hay: &[char], needle: &[char], from: usize) -> Option<(usize, usize)> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len())
        .find(|&i| {
            needle
                .iter()
                .enumerate()
                .all(|(k, &n)| same_char(hay[i + k], n))
        })
        .map(|i| (i, i + needle.len()))
}

/// Ersetzt alle Treffer, für die `accept(hay, start, end)` gilt.
fn replace_ci(
    text: &str,
    needle: &str,
    with: &str,
    accept: impl Fn(&[char], usize, usize) -> bool,
) -> String {
    let hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    let mut from = 0;
    while let Some((s, e)) = find_ci(&hay, &needle, from) {
        if accept(&hay, s, e) {
            out.extend(&hay[pos..s]);
            out.push_str(with);
            pos = e;
            from = e;
        } else {
            from = s + 1;
        }
    }
    out.extend(&hay[pos..]);
    out
}

/// Wortgrenze: kein Buchstabe und keine Ziffer direkt davor bzw. danach.
fn word_bounded(hay: &[char], s: usize, e: usize) -> bool {
    let before = s == 0 || !hay[s - 1].is_alphanumeric();
    let after = e == hay.len() || !hay[e].is_alphanumeric();
    before && after
}

/// Ende eines Pfadteils: nächster Trenner, Zeilenende oder Anführungszeichen.
fn segment_end(hay: &[char], from: usize) -> usize {
    (from..hay.len())
        .find(|&i| is_sep(hay[i]) || matches!(hay[i], '\n' | '\r' | '"' | '\'' | '<' | '>' | '|'))
        .unwrap_or(hay.len())
}

/// `…\Users\<Name>\…` und `/home/<name>/…`: der Teil nach `Users`/`home` wird ersetzt.
fn mask_user_dirs(text: &str) -> String {
    let mut hay: Vec<char> = text.chars().collect();
    for root in ["users", "home"] {
        let needle: Vec<char> = root.chars().collect();
        let mut from = 0;
        while let Some((s, e)) = find_ci(&hay, &needle, from) {
            from = s + 1;
            let sep_before = s > 0 && is_sep(hay[s - 1]);
            let sep_after = e < hay.len() && is_sep(hay[e]);
            if !(sep_before && sep_after) {
                continue;
            }
            let start = e + 1;
            let end = segment_end(&hay, start);
            let segment: String = hay[start..end].iter().collect();
            if end == start || segment == USER_MARK {
                continue;
            }
            hay.splice(start..end, USER_MARK.chars());
            from = start + USER_MARK.chars().count();
        }
    }
    hay.into_iter().collect()
}

/// `OneDrive - <Firma>` als Pfadteil → `OneDrive` (Firmenname weg).
fn mask_onedrive(text: &str) -> String {
    let mut hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = "onedrive - ".chars().collect();
    let mut from = 0;
    while let Some((s, e)) = find_ci(&hay, &needle, from) {
        from = s + 1;
        if s > 0 && !is_sep(hay[s - 1]) {
            continue;
        }
        let end = segment_end(&hay, e);
        hay.splice(s..end, "OneDrive".chars());
    }
    hay.into_iter().collect()
}

/// Anfang des Pfads, in dem Position `at` liegt (gleiche Zeile): Laufwerk `X:\`, `\\` oder
/// `/` am Wortanfang – der nächstgelegene gewinnt.
fn path_start(hay: &[char], at: usize) -> Option<usize> {
    let line_start = hay[..at]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |i| i + 1);
    let token_start = |i: usize| {
        i == line_start
            || hay[i - 1].is_whitespace()
            || matches!(hay[i - 1], '"' | '\'' | '(' | '=' | '[')
    };
    (line_start..at).rev().find(|&i| {
        let drive = i + 2 < hay.len()
            && hay[i].is_ascii_alphabetic()
            && hay[i + 1] == ':'
            && is_sep(hay[i + 2])
            && token_start(i);
        let rooted = is_sep(hay[i]) && token_start(i);
        drive || rooted
    })
}

/// Pfade, die auf einen `.pcad`-Teil führen, bis auf diesen Teil kürzen.
fn shorten_pcad_paths(text: &str) -> String {
    let mut hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = ".pcad".chars().collect();
    let mut from = 0;
    while let Some((s, e)) = find_ci(&hay, &needle, from) {
        from = e;
        let ends_part = e == hay.len() || !(hay[e].is_alphanumeric() || hay[e] == '_');
        if !ends_part {
            continue;
        }
        let comp_start = hay[..s]
            .iter()
            .rposition(|&c| is_sep(c))
            .map_or(0, |i| i + 1);
        let line_start = hay[..s]
            .iter()
            .rposition(|&c| c == '\n')
            .map_or(0, |i| i + 1);
        if comp_start <= line_start {
            continue;
        }
        if let Some(p) = path_start(&hay, comp_start) {
            hay.drain(p..comp_start);
            from = e - (comp_start - p);
        }
    }
    hay.into_iter().collect()
}

/// Wort mit genau einem `@`, Zeichen davor und danach und einem Punkt im Teil danach; der
/// letzte Teil der Domäne hat mindestens zwei Zeichen mit einem Buchstaben (`wgpu@0.19.3`
/// bleibt).
fn mask_mail(text: &str) -> String {
    let mail_char = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-' | '@');
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let w: Vec<char> = word.chars().collect();
        let a = w.iter().position(|&c| mail_char(c) && c != '@' && c != '.');
        let b = w.iter().rposition(|&c| c.is_alphanumeric());
        if let (Some(a), Some(b)) = (a, b)
            && a <= b
        {
            let core: String = w[a..=b].iter().collect();
            if let Some((local, domain)) = core.split_once('@') {
                let ok = !local.is_empty()
                    && !domain.contains('@')
                    && domain
                        .split_once('.')
                        .is_some_and(|(x, y)| !x.is_empty() && !y.is_empty())
                    && domain.rsplit('.').next().is_some_and(|tld| {
                        tld.chars().count() >= 2 && tld.chars().any(char::is_alphabetic)
                    });
                if ok {
                    out.extend(&w[..a]);
                    out.push_str(MAIL_MARK);
                    out.extend(&w[b + 1..]);
                    word.clear();
                    return;
                }
            }
        }
        out.push_str(word);
        word.clear();
    };
    for c in text.chars() {
        if c.is_whitespace() || matches!(c, ',' | ';' | '<' | '>' | '(' | ')' | '"' | '\'') {
            flush(&mut word, &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Entfernt persönliche Angaben aus einem automatisch gesammelten Text: Projektpfade werden
/// auf den letzten Teil gekürzt, Benutzername und Benutzerordner werden zu `<Benutzer>`,
/// Mailadressen zu `<Mailadresse>`.
pub fn sanitize(text: &str, who: &Personal) -> String {
    let mut t = text.to_string();
    for p in &who.project_paths {
        let full = p.to_string_lossy();
        let full = full.trim_end_matches(['\\', '/']);
        if full.is_empty() {
            continue;
        }
        // Letzter Teil, auch für Windows-Pfade auf anderen Systemen.
        let name = full.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        t = replace_ci(&t, full, &name, |_, _, _| true);
    }
    t = shorten_pcad_paths(&t);
    for h in &who.home_dirs {
        t = replace_ci(&t, h, USER_MARK, |hay, _, e| {
            e == hay.len() || !hay[e].is_alphanumeric()
        });
    }
    for n in &who.user_names {
        if n.chars().count() >= MIN_NAME_CHARS {
            t = replace_ci(&t, n, USER_MARK, word_bounded);
        }
    }
    t = mask_user_dirs(&t);
    t = mask_onedrive(&t);
    mask_mail(&t)
}

/// Höchstens `max` Zeichen; sonst gekürzt mit „…“ am Ende.
fn cut_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Zeile zur Grafikkarte für die Rückmeldung: Name und Treiber bereinigt und gekürzt
/// (Treiber höchstens [`MAX_DRIVER_CHARS`] Zeichen).
pub fn gpu_line(info: &BuildInfo, who: &Personal) -> String {
    let mut info = info.clone();
    if let Some(g) = info.gpu.as_mut() {
        g.name = cut_chars(&sanitize(&g.name, who), MAX_DRIVER_CHARS);
        g.driver = cut_chars(&sanitize(&g.driver, who), MAX_DRIVER_CHARS);
        g.backend = cut_chars(&sanitize(&g.backend, who), 40);
    }
    info.gpu_line()
}

/// Kodiert für die Adresse (UTF-8). Unverändert bleiben nur `A-Z a-z 0-9 - . _ ~`.
/// Zeilenenden werden einheitlich `%0A` (`\r\n` und ein einzelnes `\r` wie `\n`).
pub fn percent_encode(s: &str) -> String {
    let s = unify_newlines(s);
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `\r\n` und einzelnes `\r` werden zu `\n`.
fn unify_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Eine Rückmeldung. Die Texte der Bestandteile sind schon bereinigt, es stehen nur die
/// angehakten darin. Die Beschreibung bleibt, wie die Testerin sie geschrieben hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub kind: Kind,
    /// Bereich, genau einer aus [`AREAS`] (sonst schlägt in Testläufen eine Prüfung an), oder
    /// `None`. Steht als „Bereich: …“ am Anfang von `passiert` (Fehler) bzw. `angaben`
    /// (Wunsch, Frage), nicht als Parameter: GitHub füllt Auswahlfelder nicht zuverlässig vor.
    /// Das Auswahlfeld im Formular wählt die Testerin selbst.
    pub area: Option<String>,
    pub description: String,
    pub parts: Vec<(Part, String)>,
    /// Name der Projektdatei im Rückmeldungsordner (`<Name>.pcad.zip`), wenn sie mitgeht; sie
    /// steht dann im Hinweis zum Anhängen.
    pub project_file: Option<String>,
}

/// Fertige Adresse und was dafür weggelassen oder gekürzt wurde.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    pub url: String,
    pub dropped: Vec<Part>,
    pub steps_cut: bool,
    pub description_cut: bool,
    /// Wahr, wenn die Adresse auf `rueckmeldung.txt` verweist ([`ATTACH_HINT`] steht
    /// darin): `with_file` war gesetzt, oder es wurde etwas weggelassen oder gekürzt.
    pub needs_file: bool,
}

/// Hinweis auf die Dateien im Ordner: mit `bild.png`, wenn das Bild (Bestandteil `Bild`)
/// mitgeht, und mit der Projektdatei ([`Report::project_file`]).
fn attach_hint(report: &Report) -> String {
    let mut files = vec![FEEDBACK_FILE];
    if report.parts.iter().any(|(p, _)| *p == Part::Bild) {
        files.push(crate::feedback_shot::SHOT_FILE);
    }
    if let Some(p) = &report.project_file {
        files.push(p);
    }
    attach_hint_for(&files)
}

/// „Datei hier anhängen: a“ bzw. „Dateien hier anhängen: a, b und c“.
pub fn attach_hint_for(files: &[&str]) -> String {
    match files {
        [] => String::new(),
        [one] => format!("Datei hier anhängen: {one}"),
        [rest @ .., last] => format!("Dateien hier anhängen: {} und {last}", rest.join(", ")),
    }
}

fn area_line(report: &Report) -> Option<String> {
    report.area.as_ref().map(|a| format!("Bereich: {a}"))
}

fn title(report: &Report) -> String {
    let first = report.description.lines().next().unwrap_or("").trim();
    format!("{}{}", report.kind.title_prefix(), cut_chars(first, 60))
}

/// Wohin die Adresse zeigt: GitHub-Formular oder Webformular auf dennis-guse.de.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Github,
    Web,
}

fn render(
    target: Target,
    report: &Report,
    desc: &str,
    cut: bool,
    parts: &[(Part, String)],
    with_file: bool,
) -> String {
    match target {
        Target::Github => render_github(report, desc, cut, parts, with_file),
        Target::Web => render_web(report, desc, cut, parts, with_file),
    }
}

/// Baut die Webformular-Adresse. Feldnamen laut `webformular-vorgabe.md`.
fn render_web(
    report: &Report,
    desc: &str,
    cut: bool,
    parts: &[(Part, String)],
    with_file: bool,
) -> String {
    let mut url = format!(
        "{WEBFORM_URL}?art={}",
        report.kind.label().to_lowercase().replace('ä', "ae")
    );
    let mut push = |name: &str, value: &str| {
        url.push('&');
        url.push_str(name);
        url.push('=');
        url.push_str(&percent_encode(value));
    };
    if let Some(a) = &report.area {
        push("bereich", a);
    }
    for field in [
        FIELD_VERSION,
        FIELD_SYSTEM,
        FIELD_GRAFIK,
        FIELD_SCHRITTE,
        FIELD_PROTOKOLL,
    ] {
        let texts: Vec<String> = parts
            .iter()
            .filter(|(p, _)| p.bug_field() == Some(field))
            .map(|(p, t)| match field {
                FIELD_PROTOKOLL => format!("{}:\n{t}", p.label()),
                // Das Feld „system“ des Webformulars ist einzeilig.
                FIELD_SYSTEM => t.lines().collect::<Vec<_>>().join(", "),
                _ => t.clone(),
            })
            .collect();
        if !texts.is_empty() {
            push(field, &texts.join("\n\n"));
        }
    }
    let mut description = desc.to_string();
    if cut {
        description.push_str(KUERZUNG_HINWEIS);
    }
    if with_file {
        description.push_str("\n\n");
        description.push_str(&attach_hint(report));
    }
    push(report.kind.description_field(), &description);
    url
}

/// Baut die Adresse aus der (evtl. gekürzten) Beschreibung und den Bestandteilen.
fn render_github(
    report: &Report,
    desc: &str,
    cut: bool,
    parts: &[(Part, String)],
    with_file: bool,
) -> String {
    let mut fields: Vec<(&str, String)> = Vec::new();
    let mut description = String::new();
    let area = area_line(report);
    match report.kind {
        Kind::Fehler => {
            if let Some(a) = &area {
                description.push_str(a);
                description.push_str("\n\n");
            }
            for field in [
                FIELD_VERSION,
                FIELD_SYSTEM,
                FIELD_GRAFIK,
                FIELD_SCHRITTE,
                FIELD_PROTOKOLL,
            ] {
                let texts: Vec<String> = parts
                    .iter()
                    .filter(|(p, _)| p.bug_field() == Some(field))
                    .map(|(p, t)| {
                        if field == FIELD_PROTOKOLL {
                            format!("{}:\n{t}", p.label())
                        } else {
                            t.clone()
                        }
                    })
                    .collect();
                if !texts.is_empty() {
                    fields.push((field, texts.join("\n\n")));
                }
            }
        }
        Kind::Wunsch | Kind::Frage => {
            let mut blocks: Vec<String> = area.into_iter().collect();
            blocks.extend(
                parts
                    .iter()
                    .filter(|(p, _)| p.bug_field().is_some())
                    .map(|(p, t)| format!("{}:\n{t}", p.label())),
            );
            if !blocks.is_empty() {
                fields.push((FIELD_ANGABEN, blocks.join("\n\n")));
            }
        }
    }
    description.push_str(desc);
    if cut {
        description.push_str(KUERZUNG_HINWEIS);
    }
    if with_file {
        description.push_str("\n\n");
        description.push_str(&attach_hint(report));
    }
    // Die Beschreibung steht zuletzt.
    fields.push((report.kind.description_field(), description));

    let mut url = format!(
        "https://github.com/{}/issues/new?template={}&title={}",
        planora_update::REPO,
        report.kind.template(),
        percent_encode(&title(report))
    );
    for (name, value) in fields {
        url.push('&');
        url.push_str(name);
        url.push('=');
        url.push_str(&percent_encode(&value));
    }
    url
}

/// Adresse des GitHub-Formulars, vorausgefüllt, immer kürzer als [`MAX_URL_LEN`].
///
/// Ist sie zu lang, fallen zuerst automatisch gesammelte Angaben weg: die Protokollzeilen,
/// dann die Schritte von den ältesten her, dann Absturzbericht, Grafikkarte, System und
/// Version. Die Beschreibung bleibt ganz, solange sie allein passt; sonst wird sie an einer
/// Zeichengrenze gekürzt und endet mit [`KUERZUNG_HINWEIS`].
///
/// `with_file` hängt [`ATTACH_HINT`] an das Beschreibungsfeld. Wurde etwas weggelassen oder
/// gekürzt, geschieht das von selbst; [`Built::needs_file`] sagt dann, dass der Ordner mit
/// `rueckmeldung.txt` angelegt werden muss.
pub fn issue_url(report: &Report, with_file: bool) -> Built {
    build(Target::Github, report, with_file)
}

/// Adresse des Webformulars auf dennis-guse.de (Weg ohne GitHub-Konto), vorausgefüllt,
/// immer kürzer als [`MAX_URL_LEN`]. Art, Bereich, Version, System und Grafikkarte bleiben;
/// bei Platzmangel fallen zuerst Protokoll und Absturzbericht, dann Schritte weg, zuletzt
/// wird die Beschreibung gekürzt. Sonst wie [`issue_url`].
pub fn webform_url(report: &Report, with_file: bool) -> Built {
    build(Target::Web, report, with_file)
}

fn build(target: Target, report: &Report, with_file: bool) -> Built {
    debug_assert!(
        report.area.as_deref().is_none_or(|a| AREAS.contains(&a)),
        "unbekannter Bereich {:?}",
        report.area
    );
    let mut parts: Vec<(Part, String)> = report.parts.clone();
    let mut built = Built {
        url: String::new(),
        dropped: Vec::new(),
        steps_cut: false,
        description_cut: false,
        needs_file: with_file,
    };
    let fits = |parts: &[(Part, String)], attach: bool| {
        let url = render(target, report, &report.description, false, parts, attach);
        (url.len() < MAX_URL_LEN).then_some(url)
    };
    if let Some(url) = fits(&parts, with_file) {
        built.url = url;
        return built;
    }
    // Ab hier fehlt in der Adresse etwas: der Datei-Hinweis gehört immer dazu.
    built.needs_file = true;
    // 1. Protokollzeilen weg
    if parts.iter().any(|(p, _)| *p == Part::Protokoll) {
        parts.retain(|(p, _)| *p != Part::Protokoll);
        built.dropped.push(Part::Protokoll);
        if let Some(url) = fits(&parts, true) {
            built.url = url;
            return built;
        }
    }
    // 2. Schritte von den ältesten her
    if let Some(i) = parts.iter().position(|(p, _)| *p == Part::Schritte) {
        built.steps_cut = true;
        loop {
            let lines: Vec<&str> = parts[i].1.lines().collect();
            if lines.len() <= 1 {
                parts.remove(i);
                built.dropped.push(Part::Schritte);
                break;
            }
            parts[i].1 = lines[1..].join("\n");
            if let Some(url) = fits(&parts, true) {
                built.url = url;
                return built;
            }
        }
    }
    // 3. übrige automatische Angaben, solange die ganze Beschreibung nicht passt
    //    (im Webformular bleiben Version, System und Grafikkarte immer stehen)
    let extras: &[Part] = match target {
        Target::Github => &[
            Part::Absturzbericht,
            Part::Grafik,
            Part::System,
            Part::Version,
        ],
        Target::Web => &[Part::Absturzbericht],
    };
    for &p in extras {
        if let Some(url) = fits(&parts, true) {
            built.url = url;
            return built;
        }
        if parts.iter().any(|(q, _)| *q == p) {
            parts.retain(|(q, _)| *q != p);
            built.dropped.push(p);
        }
    }
    if let Some(url) = fits(&parts, true) {
        built.url = url;
        return built;
    }
    // 3b. Webformular, letzte Rettung: passt selbst die leere Beschreibung nicht, fallen auch
    //     Grafikkarte, System und Version weg (nur bei unsinnig langen Angaben).
    if target == Target::Web {
        for p in [Part::Grafik, Part::System, Part::Version] {
            if render(target, report, "", true, &parts, true).len() < MAX_URL_LEN {
                break;
            }
            if parts.iter().any(|(q, _)| *q == p) {
                parts.retain(|(q, _)| *q != p);
                built.dropped.push(p);
            }
        }
    }
    if let Some(url) = fits(&parts, true) {
        built.url = url;
        return built;
    }
    // 4. Die Beschreibung passt auch allein nicht: an einer Zeichengrenze kürzen.
    let chars: Vec<char> = report.description.chars().collect();
    let with_desc = |n: usize| {
        let desc: String = chars[..n].iter().collect();
        render(target, report, &desc, true, &parts, true)
    };
    // größtes n mit passender Adresse (Länge wächst mit n)
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if with_desc(mid).len() < MAX_URL_LEN {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    built.description_cut = true;
    built.url = with_desc(lo);
    built
}

/// Alles ungekürzt, mit Überschrift je Teil. Inhalt von `rueckmeldung.txt`; Zeilenenden
/// überall `\n` (auch unter Windows; der Editor dort zeigt das richtig an).
pub fn full_text(report: &Report) -> String {
    let mut out = format!("Art: {}\n", report.kind.label());
    if let Some(a) = area_line(report) {
        out.push_str(&a);
        out.push('\n');
    }
    out.push_str("\n== Beschreibung ==\n");
    out.push_str(&report.description);
    out.push('\n');
    for (p, t) in &report.parts {
        out.push_str(&format!("\n== {} ==\n{t}\n", p.label()));
    }
    unify_newlines(&out)
}

/// Neuer Ordner `rueckmeldungen/<Datum_Uhrzeit>` im Datenordner; gibt es ihn schon, mit
/// `_2`, `_3` … Der Ordner wird nicht angelegt.
pub fn bundle_dir(data_dir: &Path, now: chrono::NaiveDateTime) -> PathBuf {
    let base = data_dir.join(FEEDBACK_DIR);
    let stamp = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let first = base.join(&stamp);
    if !first.exists() {
        return first;
    }
    (2u32..)
        .map(|i| base.join(format!("{stamp}_{i}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Wie lange ein Rückmeldungsordner höchstens bleibt (Tage), wie bei den Absturzberichten.
pub const FEEDBACK_KEEP_DAYS: i64 = 30;
/// So viele Rückmeldungsordner bleiben höchstens.
pub const FEEDBACK_KEEP_MAX: usize = 10;

/// Zeitpunkt aus dem Ordnernamen `<Datum_Uhrzeit>` oder `<Datum_Uhrzeit>_<n>` (siehe
/// [`bundle_dir`]) samt Zähler; fremde Namen ergeben `None`.
fn bundle_stamp(name: &str) -> Option<(chrono::NaiveDateTime, u32)> {
    let (stamp, n) = match name.get(19..) {
        None | Some("") => (name, 1),
        Some(rest) => (&name[..19], rest.strip_prefix('_')?.parse::<u32>().ok()?),
    };
    let t = chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d_%H-%M-%S").ok()?;
    Some((t, n))
}

/// Räumt `<Datenordner>/rueckmeldungen` auf: Unterordner, die älter als [`FEEDBACK_KEEP_DAYS`]
/// Tage sind, und danach alle bis auf die [`FEEDBACK_KEEP_MAX`] neuesten werden gelöscht. Das
/// Alter steht im Ordnernamen. Nur eigene Ordner (Namensmuster von [`bundle_dir`]); Dateien und
/// fremde Ordner bleiben, Fehler werden übergangen.
pub fn cleanup_feedback_dirs(base: &Path, now: chrono::NaiveDateTime) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut own: Vec<((chrono::NaiveDateTime, u32), PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| Some((bundle_stamp(e.file_name().to_str()?)?, e.path())))
        .collect();
    own.sort_by_key(|a| std::cmp::Reverse(a.0));
    let max_age = chrono::Duration::days(FEEDBACK_KEEP_DAYS);
    for (i, ((time, _), path)) in own.iter().enumerate() {
        if i >= FEEDBACK_KEEP_MAX || now.signed_duration_since(*time) > max_age {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Legt den Ordner an und schreibt [`full_text`] nach `rueckmeldung.txt`.
pub fn write_bundle(dir: &Path, report: &Report) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FEEDBACK_FILE);
    std::fs::write(&path, full_text(report))?;
    Ok(path)
}

/// Öffnet einen Ordner im Dateimanager; in Tests eine Nachbildung.
pub trait SystemOpen {
    /// Wahr, wenn der Dateimanager gestartet werden konnte.
    fn open_folder(&self, dir: &Path) -> bool;
}

/// Windows-Explorer bzw. `xdg-open`.
pub struct ExplorerOpen;

impl SystemOpen for ExplorerOpen {
    fn open_folder(&self, dir: &Path) -> bool {
        let program = if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program).arg(dir).spawn().is_ok()
    }
}

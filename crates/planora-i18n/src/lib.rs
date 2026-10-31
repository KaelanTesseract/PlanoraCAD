//! Sprachen für PlanoraCAD: Sprachdateien (Fluent, `.ftl`), aktuelle Sprache und Zahlenformat.
//!
//! Deutsch und Englisch sind eingebaut; weitere Sprachen liest [`load_dir`] aus einem Ordner.
//! Texte holt man mit [`tr!`], [`t`] oder [`t_args`]. Fehlt ein Text in der aktuellen Sprache,
//! kommt der englische; fehlt er auch dort, die Id selbst. Nie ein Absturz.
//!
//! Die aktuelle Sprache ist die Überschreibung des Threads ([`with_lang`]), sonst die globale
//! ([`set_global`]); vor dem ersten `set_global` gilt Deutsch.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use fluent_syntax::ast::{
    CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
};
use unic_langid::LanguageIdentifier;

const DE_FTL: &str = include_str!("../languages/de.ftl");
const EN_FTL: &str = include_str!("../languages/en.ftl");

// ---- Sprache --------------------------------------------------------------------------------

struct LangData {
    id: String,
    name: String,
    decimal: char,
    bundle: FluentBundle<FluentResource>,
}

/// Eine Sprache mit ihren Texten. Billig zu klonen (gemeinsame Daten hinter `Arc`).
#[derive(Clone)]
pub struct Lang(Arc<LangData>);

impl Lang {
    /// Sprach-Id (BCP 47), z. B. `de`, `en`, `pt-BR`.
    pub fn id(&self) -> &str {
        &self.0.id
    }

    /// Eigenname der Sprache aus `language-name`, z. B. „Deutsch“.
    pub fn name(&self) -> String {
        self.0.name.clone()
    }

    /// Dezimaltrenner aus `decimal-separator` (`,` oder `.`).
    pub fn decimal(&self) -> char {
        self.0.decimal
    }

    /// Text zur Id nur aus dieser Sprache, ohne Rückfall.
    fn format(&self, id: &str, args: Option<&FluentArgs>) -> Option<String> {
        format_in(&self.0.bundle, id, args)
    }
}

impl fmt::Debug for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lang")
            .field("id", &self.0.id)
            .field("name", &self.0.name)
            .field("decimal", &self.0.decimal)
            .finish()
    }
}

/// Baut eine Sprache aus dem Inhalt einer `.ftl`-Datei. Fehlertexte englisch.
fn build(id: &str, source: String) -> Result<Lang, String> {
    let langid: LanguageIdentifier = id
        .parse()
        .map_err(|_| format!("\"{id}\" is not a valid language id (BCP 47, e.g. fr or pt-BR)"))?;
    let resource = FluentResource::try_new(source).map_err(|(_, errors)| {
        let first = errors.first().map(|e| format!("{e}")).unwrap_or_default();
        format!("syntax error ({} in total): {first}", errors.len())
    })?;
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    bundle.set_use_isolating(false);
    bundle.add_resource(resource).map_err(|errors| {
        let first = errors.first().map(|e| format!("{e}")).unwrap_or_default();
        format!("invalid entries ({} in total): {first}", errors.len())
    })?;
    let mut data = LangData {
        id: id.to_string(),
        name: String::new(),
        decimal: '.',
        bundle,
    };
    let name = format_in(&data.bundle, "language-name", None)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "the entry \"language-name\" is missing".to_string())?;
    data.name = name;
    if let Some(sep) = format_in(&data.bundle, "decimal-separator", None) {
        data.decimal = match sep.trim() {
            "," => ',',
            "." => '.',
            other => {
                return Err(format!(
                    "\"decimal-separator\" must be \",\" or \".\", not \"{other}\""
                ));
            }
        };
    }
    Ok(Lang(Arc::new(data)))
}

/// Text zur Id aus einem Bundle; `None`, wenn die Nachricht oder ihr Wert fehlt.
fn format_in(
    bundle: &FluentBundle<FluentResource>,
    id: &str,
    args: Option<&FluentArgs>,
) -> Option<String> {
    let pattern = bundle.get_message(id)?.value()?;
    let mut errors = Vec::new();
    Some(
        bundle
            .format_pattern(pattern, args, &mut errors)
            .into_owned(),
    )
}

// ---- Verzeichnis der Sprachen ---------------------------------------------------------------

struct Registry {
    german: Lang,
    english: Lang,
    /// Aus Dateien geladene Sprachen (nie `de`/`en`).
    loaded: RwLock<Vec<Lang>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        german: build("de", DE_FTL.to_string()).expect("built-in de.ftl is valid"),
        english: build("en", EN_FTL.to_string()).expect("built-in en.ftl is valid"),
        loaded: RwLock::new(Vec::new()),
    })
}

/// Die eingebaute deutsche Sprache.
pub fn german() -> Lang {
    registry().german.clone()
}

/// Die eingebaute englische Sprache.
pub fn english() -> Lang {
    registry().english.clone()
}

/// Alle Sprachen (eingebaute und geladene), nach Eigenname sortiert.
pub fn available() -> Vec<Lang> {
    let reg = registry();
    let mut all = vec![reg.german.clone(), reg.english.clone()];
    all.extend(
        reg.loaded
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned(),
    );
    all.sort_by_key(|l| l.name().to_lowercase());
    all
}

/// Sprache zur Id (Groß-/Kleinschreibung egal).
pub fn find(id: &str) -> Option<Lang> {
    available()
        .into_iter()
        .find(|l| l.id().eq_ignore_ascii_case(id))
}

/// Ein Problem beim Laden einer Sprachdatei.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadProblem {
    pub file: PathBuf,
    /// Beschreibung auf Englisch (fürs Protokoll).
    pub message: String,
    /// `true`: die Datei wurde übergangen; `false`: sie wurde geladen, aber ein Text hat andere
    /// Platzhalter als im Englischen (dort fehlt dann womöglich ein Wert).
    pub skipped: bool,
}

// ---- Platzhalter ----------------------------------------------------------------------------

/// Platzhalter (`$name`, ohne `$`) je Schlüssel einer `.ftl`-Datei, in Reihenfolge der Datei.
/// Schlüssel: `id`, `id.attribut`, Terme mit `-` davor (`-id`, `-id.attribut`); doppelte
/// Schlüssel kommen doppelt vor. `None` bei Syntaxfehlern.
pub fn placeholders(source: &str) -> Option<Vec<(String, BTreeSet<String>)>> {
    let resource = fluent_syntax::parser::parse(source).ok()?;
    let mut out = Vec::new();
    for entry in &resource.body {
        match entry {
            Entry::Message(m) => {
                out.push((
                    m.id.name.to_string(),
                    m.value.as_ref().map(vars_of).unwrap_or_default(),
                ));
                for a in &m.attributes {
                    out.push((format!("{}.{}", m.id.name, a.id.name), vars_of(&a.value)));
                }
            }
            Entry::Term(t) => {
                out.push((format!("-{}", t.id.name), vars_of(&t.value)));
                for a in &t.attributes {
                    out.push((format!("-{}.{}", t.id.name, a.id.name), vars_of(&a.value)));
                }
            }
            _ => {}
        }
    }
    Some(out)
}

fn vars_of(p: &Pattern<&str>) -> BTreeSet<String> {
    let mut vars = BTreeSet::new();
    vars_in_pattern(p, &mut vars);
    vars
}

fn vars_in_pattern(p: &Pattern<&str>, out: &mut BTreeSet<String>) {
    for el in &p.elements {
        if let PatternElement::Placeable { expression } = el {
            vars_in_expression(expression, out);
        }
    }
}

fn vars_in_expression(e: &Expression<&str>, out: &mut BTreeSet<String>) {
    match e {
        Expression::Inline(i) => vars_in_inline(i, out),
        Expression::Select { selector, variants } => {
            vars_in_inline(selector, out);
            for v in variants {
                vars_in_pattern(&v.value, out);
            }
        }
    }
}

fn vars_in_args(a: &CallArguments<&str>, out: &mut BTreeSet<String>) {
    for p in &a.positional {
        vars_in_inline(p, out);
    }
    for n in &a.named {
        vars_in_inline(&n.value, out);
    }
}

fn vars_in_inline(i: &InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match i {
        InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_string());
        }
        InlineExpression::FunctionReference { arguments, .. } => vars_in_args(arguments, out),
        InlineExpression::TermReference {
            arguments: Some(a), ..
        } => vars_in_args(a, out),
        InlineExpression::Placeable { expression } => vars_in_expression(expression, out),
        _ => {}
    }
}

/// Platzhalter je Nachricht und Attribut (ohne Terme) der eingebauten englischen Datei.
fn english_placeholders() -> &'static BTreeMap<String, BTreeSet<String>> {
    static EN: OnceLock<BTreeMap<String, BTreeSet<String>>> = OnceLock::new();
    EN.get_or_init(|| {
        placeholders(EN_FTL)
            .expect("built-in en.ftl is valid")
            .into_iter()
            .filter(|(key, _)| !key.starts_with('-'))
            .collect()
    })
}

/// Englische Beschreibungen aller Nachrichten und Attribute aus `source`, deren Platzhalter
/// nicht genau die des Englischen sind (Schlüssel, die es im Englischen nicht gibt, zählen nicht).
fn placeholder_mismatches(source: &str) -> Vec<String> {
    let english = english_placeholders();
    let show = |vars: &BTreeSet<String>| {
        if vars.is_empty() {
            "none".to_string()
        } else {
            vars.iter()
                .map(|v| format!("${v}"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    placeholders(source)
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, _)| !key.starts_with('-'))
        .filter_map(|(key, vars)| {
            let expected = english.get(&key)?;
            (vars != *expected).then(|| {
                format!(
                    "\"{key}\" uses the placeholders {} but English uses {}",
                    show(&vars),
                    show(expected)
                )
            })
        })
        .collect()
}

/// Lädt alle `*.ftl` aus `dir` als zusätzliche Sprachen (Id = Dateiname ohne `.ftl`, `_` wird
/// zu `-`, also `pt_BR.ftl` → `pt-BR`). Fehlerhafte Dateien werden übergangen und als Problem
/// gemeldet. Texte mit anderen Platzhaltern als im Englischen werden als Problem gemeldet, die
/// Datei wird trotzdem geladen. Eine Datei mit der Id einer eingebauten Sprache ersetzt diese
/// nicht. Fehlt der Ordner, gibt es keine Probleme.
pub fn load_dir(dir: &Path) -> Vec<LoadProblem> {
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return problems;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ftl"))
        })
        .collect();
    files.sort();
    let mut fresh = Vec::new();
    for file in files {
        let problem = |message: String| LoadProblem {
            file: file.clone(),
            message,
            skipped: true,
        };
        let Some(id) = file
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.replace('_', "-"))
        else {
            problems.push(problem("the file name is not valid UTF-8".to_string()));
            continue;
        };
        if id.eq_ignore_ascii_case("de") || id.eq_ignore_ascii_case("en") {
            problems.push(problem(format!(
                "\"{id}\" is built in and cannot be replaced by a file"
            )));
            continue;
        }
        let source = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                problems.push(problem(format!("cannot read the file: {e}")));
                continue;
            }
        };
        let mismatches = placeholder_mismatches(&source);
        match build(&id, source) {
            Ok(lang) => {
                fresh.push(lang);
                problems.extend(mismatches.into_iter().map(|message| LoadProblem {
                    skipped: false,
                    ..problem(message)
                }));
            }
            Err(message) => problems.push(problem(message)),
        }
    }
    let mut loaded = registry().loaded.write().unwrap_or_else(|e| e.into_inner());
    for lang in fresh {
        loaded.retain(|l| !l.id().eq_ignore_ascii_case(lang.id()));
        loaded.push(lang);
    }
    problems
}

/// Wählt die Sprache: gespeicherte Id, wenn es sie gibt; sonst die erste Systemsprache, deren
/// volle Id oder erstes Subtag passt (`de-DE` → `de`); sonst Englisch.
pub fn pick(saved: Option<&str>, system: &[String]) -> Lang {
    if let Some(lang) = saved.and_then(find) {
        return lang;
    }
    for sys in system {
        if let Some(lang) = find(sys) {
            return lang;
        }
        let primary = sys.split(['-', '_']).next().unwrap_or("");
        if !primary.is_empty()
            && let Some(lang) = find(primary)
        {
            return lang;
        }
    }
    english()
}

// ---- Aktuelle Sprache -----------------------------------------------------------------------

static GLOBAL: RwLock<Option<Lang>> = RwLock::new(None);

thread_local! {
    static OVERRIDE: RefCell<Option<Lang>> = const { RefCell::new(None) };
}

/// Die Sprache dieses Threads: Überschreibung aus [`with_lang`], sonst die globale.
pub fn current() -> Lang {
    if let Some(lang) = OVERRIDE.with(|o| o.borrow().clone()) {
        return lang;
    }
    GLOBAL
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(german)
}

/// Setzt die globale Sprache (nur in `main` und einem eigenen Test).
pub fn set_global(lang: Lang) {
    *GLOBAL.write().unwrap_or_else(|e| e.into_inner()) = Some(lang);
}

/// Führt `f` in diesem Thread mit `lang` aus; verschachtelbar. Danach gilt wieder die vorige
/// Sprache, auch wenn `f` in Panik gerät.
pub fn with_lang<R>(lang: &Lang, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Lang>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            OVERRIDE.with(|o| *o.borrow_mut() = previous);
        }
    }
    let previous = OVERRIDE.with(|o| o.borrow_mut().replace(lang.clone()));
    let _restore = Restore(previous);
    f()
}

// ---- Maßeinheit -----------------------------------------------------------------------------

/// Einheit, in der Längen angezeigt und eingegeben werden. Intern bleibt alles in Millimetern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DisplayUnit {
    #[default]
    Millimeter,
    Zoll,
}

impl DisplayUnit {
    /// Kennung zum Speichern: `"mm"` oder `"in"`.
    pub fn id(self) -> &'static str {
        match self {
            DisplayUnit::Millimeter => "mm",
            DisplayUnit::Zoll => "in",
        }
    }

    /// Einheit zur Kennung; unbekannte Kennungen geben `None`.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "mm" => Some(DisplayUnit::Millimeter),
            "in" => Some(DisplayUnit::Zoll),
            _ => None,
        }
    }

    /// Wie viele Millimeter eine Einheit hat.
    pub fn mm_per_unit(self) -> f64 {
        match self {
            DisplayUnit::Millimeter => 1.0,
            DisplayUnit::Zoll => 25.4,
        }
    }

    /// Kurzzeichen der Länge.
    pub fn symbol(self) -> &'static str {
        self.id()
    }

    /// Kurzzeichen der Fläche.
    pub fn area_symbol(self) -> &'static str {
        match self {
            DisplayUnit::Millimeter => "mm²",
            DisplayUnit::Zoll => "in²",
        }
    }

    /// Kurzzeichen des Volumens.
    pub fn volume_symbol(self) -> &'static str {
        match self {
            DisplayUnit::Millimeter => "mm³",
            DisplayUnit::Zoll => "in³",
        }
    }

    /// Standard-Nachkommastellen der Anzeige.
    pub fn decimals(self) -> usize {
        match self {
            DisplayUnit::Millimeter => 2,
            DisplayUnit::Zoll => 3,
        }
    }

    /// Millimeter -> Anzeigewert.
    pub fn to_display(self, mm: f64) -> f64 {
        mm / self.mm_per_unit()
    }

    /// Anzeigewert -> Millimeter.
    pub fn from_display(self, v: f64) -> f64 {
        v * self.mm_per_unit()
    }
}

static GLOBAL_UNIT: RwLock<DisplayUnit> = RwLock::new(DisplayUnit::Millimeter);

thread_local! {
    static UNIT_OVERRIDE: RefCell<Option<DisplayUnit>> = const { RefCell::new(None) };
}

/// Die Einheit dieses Threads: Überschreibung aus [`with_unit`], sonst die globale.
pub fn current_unit() -> DisplayUnit {
    if let Some(unit) = UNIT_OVERRIDE.with(|o| *o.borrow()) {
        return unit;
    }
    *GLOBAL_UNIT.read().unwrap_or_else(|e| e.into_inner())
}

/// Setzt die globale Einheit (nur in `main`, bei der Einstellung und in einem eigenen Test).
pub fn set_global_unit(unit: DisplayUnit) {
    *GLOBAL_UNIT.write().unwrap_or_else(|e| e.into_inner()) = unit;
}

/// Führt `f` in diesem Thread mit `unit` aus; verschachtelbar. Danach gilt wieder die vorige
/// Einheit, auch wenn `f` in Panik gerät.
pub fn with_unit<R>(unit: DisplayUnit, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<DisplayUnit>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            UNIT_OVERRIDE.with(|o| *o.borrow_mut() = previous);
        }
    }
    let previous = UNIT_OVERRIDE.with(|o| o.borrow_mut().replace(unit));
    let _restore = Restore(previous);
    f()
}

/// Länge (in mm) als Zahl in der aktuellen Einheit, ohne Kurzzeichen.
pub fn fmt_len_num(mm: f64) -> String {
    let unit = current_unit();
    fmt_num(unit.to_display(mm), unit.decimals())
}

/// Länge (in mm) mit Kurzzeichen, z. B. „12,50 mm“ oder „0,492 in“.
pub fn fmt_len(mm: f64) -> String {
    format!("{} {}", fmt_len_num(mm), current_unit().symbol())
}

/// Wie [`fmt_len`], aber mit eigener Stellenzahl für Millimeter; in Zoll kommt eine Stelle dazu.
pub fn fmt_len_with(mm: f64, decimals_mm: usize) -> String {
    let unit = current_unit();
    let decimals = match unit {
        DisplayUnit::Millimeter => decimals_mm,
        DisplayUnit::Zoll => decimals_mm + 1,
    };
    format!(
        "{} {}",
        fmt_num(unit.to_display(mm), decimals),
        unit.symbol()
    )
}

/// Fläche (in mm²) mit Kurzzeichen.
pub fn fmt_area(mm2: f64) -> String {
    let unit = current_unit();
    let f = unit.mm_per_unit();
    format!(
        "{} {}",
        fmt_num(mm2 / (f * f), unit.decimals()),
        unit.area_symbol()
    )
}

/// Volumen (in mm³) mit Kurzzeichen.
pub fn fmt_volume(mm3: f64) -> String {
    let unit = current_unit();
    let f = unit.mm_per_unit();
    format!(
        "{} {}",
        fmt_num(mm3 / (f * f * f), unit.decimals()),
        unit.volume_symbol()
    )
}

// ---- Texte ----------------------------------------------------------------------------------

/// Wert für einen Platzhalter. Kommazahlen als Text übergeben (siehe [`fmt_num`]);
/// ganze Zahlen als `Int` (für die Mehrzahl).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg<'a> {
    Text(Cow<'a, str>),
    Int(i64),
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(v: &'a str) -> Self {
        Arg::Text(Cow::Borrowed(v))
    }
}

impl<'a> From<&'a String> for Arg<'a> {
    fn from(v: &'a String) -> Self {
        Arg::Text(Cow::Borrowed(v.as_str()))
    }
}

impl From<String> for Arg<'_> {
    fn from(v: String) -> Self {
        Arg::Text(Cow::Owned(v))
    }
}

impl<'a> From<Cow<'a, str>> for Arg<'a> {
    fn from(v: Cow<'a, str>) -> Self {
        Arg::Text(v)
    }
}

macro_rules! arg_from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Arg<'_> {
            fn from(v: $t) -> Self {
                Arg::Int(i64::try_from(v).unwrap_or(i64::MAX))
            }
        }
    )*};
}
arg_from_int!(i64, i32, u32, u64, usize);

/// Text zur Id in der aktuellen Sprache (Rückfall: Englisch, dann die Id).
pub fn t(id: &str) -> String {
    lookup(id, None)
}

/// Text zur Id mit Platzhaltern in der aktuellen Sprache (Rückfall: Englisch, dann die Id).
pub fn t_args(id: &str, args: &[(&str, Arg)]) -> String {
    let mut fargs = FluentArgs::new();
    for (key, value) in args {
        let v: FluentValue = match value {
            Arg::Text(s) => FluentValue::from(s.as_ref()),
            Arg::Int(n) => FluentValue::from(*n),
        };
        fargs.set(*key, v);
    }
    lookup(id, Some(&fargs))
}

fn lookup(id: &str, args: Option<&FluentArgs>) -> String {
    let lang = current();
    lang.format(id, args)
        .or_else(|| english().format(id, args))
        .unwrap_or_else(|| id.to_string())
}

/// Text in der aktuellen Sprache: `tr!("id")` oder `tr!("id", name = wert, anzahl = 3)`.
#[macro_export]
macro_rules! tr {
    ($id:literal) => {
        $crate::t($id)
    };
    ($id:literal, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::t_args($id, &[$((stringify!($k), $crate::Arg::from($v))),+])
    };
}

// ---- Zahlen ---------------------------------------------------------------------------------

/// Zahl mit `decimals` Nachkommastellen und dem Trenner der aktuellen Sprache (nur Anzeige).
/// Wie `sketch_view::format_number`: kein „-0,00“.
pub fn fmt_num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = match s.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_string(),
        _ => s,
    };
    localize_decimal(&s)
}

/// Ersetzt `.` durch den Dezimaltrenner der aktuellen Sprache.
pub fn localize_decimal(s: &str) -> String {
    let sep = current().decimal();
    if sep == '.' {
        s.to_string()
    } else {
        s.replace('.', &sep.to_string())
    }
}

/// Liest eine Zahl; Komma und Punkt gelten als Dezimaltrenner. Nicht endliche Werte sind ungültig.
pub fn parse_num(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

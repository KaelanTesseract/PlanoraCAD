//! Fenster „Rückmeldung“ (Hilfe → Rückmeldung …, Strg+Umschalt+R): Art (Fehler, Wunsch,
//! Frage), Bereich und Beschreibung, darunter jeder automatisch gesammelte Bestandteil mit
//! Häkchen und aufklappbarer, schon bereinigter Vorschau, dann die fertige Adresse zum
//! Aufklappen. „Im Browser öffnen“ legt bei Bedarf den Rückmeldungsordner mit
//! `rueckmeldung.txt` an und öffnet ihn, danach die Adresse. Das Fenster bleibt dann offen und
//! hilft weiter, falls sich kein Browser geöffnet hat („Text kopieren“).
//!
//! Das Fenster rechnet nichts selbst: Bereinigung, Adresse und Kürzung kommen aus
//! [`crate::feedback`]. Was nach außen geht (Browser, Dateimanager, Zwischenablage), gibt es als
//! [`FeedbackAction`] an die App, die es ausführt. Über das Netz geht nichts, bevor die
//! Testerin im Browser selbst absendet.
//!
//! Welche Fenster es schließt und welche es schließen, regelt die App (`app::open_feedback`,
//! `app::enforce_feedback_rules`).

use crate::build_info::BuildInfo;
use crate::diag::{self, Step};
use crate::dialog::{Dialog, DialogSize};
use crate::feedback::{self, AREAS, Built, FEEDBACK_FILE, Kind, Part, Personal, Report};
use crate::feedback_shot::{SHOT_FAILED, ShotFlow, ShotPhase, ShotStep, preview_line, save_png};
use crate::theme::{self, SPACE_1, SPACE_2, SPACE_3, SPACE_4};
use std::cell::Cell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// Titel des Fensters.
pub const TITLE: &str = "Rückmeldung";
/// Vor der Wahl von Fehler, Wunsch oder Frage.
pub const KIND_LABEL: &str = "Worum geht es?";
/// Vor der Auswahl des Bereichs (freiwillig, steht als „Bereich: …“ am Anfang des Textes).
pub const AREA_LABEL: &str = "Bereich";
/// Auswahl „kein Bereich“.
pub const AREA_NONE: &str = "Keine Angabe";
/// Über dem Beschreibungsfeld.
pub const DESCRIPTION_LABEL: &str = "Beschreib kurz, was du gemacht hast und was passiert ist.";
/// So viele Zeichen braucht die Beschreibung mindestens (ohne Leerraum am Rand).
pub const MIN_CHARS: usize = 10;
/// Neben dem gesperrten Knopf, solange die Beschreibung zu kurz ist.
pub const TOO_SHORT: &str = "Bitte schreib mindestens 10 Zeichen.";
/// Überschrift über den Bestandteilen mit Häkchen.
pub const PREVIEW_HEADING: &str = "Das geht mit";
/// Hauptknopf: Ordner (bei Bedarf) und Adresse öffnen.
pub const OPEN_BROWSER: &str = "Im Browser öffnen";
/// Schließt ohne etwas zu tun.
pub const CANCEL: &str = "Abbrechen";
/// Schließt nach „Im Browser öffnen“.
pub const CLOSE: &str = "Schließen";
/// Kopf der aufklappbaren Adresse.
pub const SHOW_ADDRESS: &str = "Adresse anzeigen";
/// Kopiert die Adresse in die Zwischenablage.
pub const COPY_TEXT: &str = "Text kopieren";
/// Nach „Im Browser öffnen“.
pub const BROWSER_HINT: &str =
    "Hat sich kein Browser geöffnet? Kopier die Adresse und füg sie im Browser ein.";
/// Nach „Text kopieren“.
pub const COPIED: &str = "Kopiert.";
/// Statt [`folder_hint`], wenn der Rückmeldungsordner nicht angelegt werden konnte.
pub const NO_FILE: &str = "Die Datei ließ sich nicht anlegen. Die Meldung geht trotzdem.";
/// Steht immer unten im Fenster.
pub const PRIVACY_LINE: &str = "Es geht nichts weg, bevor du im Browser auf „Absenden“ klickst.";
/// So viele letzte Protokollzeilen kommen in den Bestandteil „Protokollzeilen“.
pub const LOG_LINES: usize = 100;
/// Vorschau zum Bild, solange noch keins aufgenommen ist.
pub const SHOT_NOT_YET: &str =
    "Beim Anhaken nimmt das Programm ein Bild vom Fenster auf (ohne dieses Fenster).";

/// Kennung des Fensters.
const WINDOW_ID: &str = "planora_feedback";
/// Größte Höhe einer Vorschau; darüber rollt sie.
const PREVIEW_MAX_H: f32 = 160.0;
/// Größte Höhe der aufgeklappten Adresse; darüber rollt sie.
const ADDRESS_MAX_H: f32 = 120.0;
/// Zeilen des Beschreibungsfelds (es wächst mit dem Text).
const DESCRIPTION_ROWS: usize = 4;

/// Hinweis nach „Im Browser öffnen“, wenn ein Ordner mit `file` geöffnet wurde.
pub fn folder_hint(file: &str) -> String {
    format!("Zieh „{file}“ aus dem geöffneten Ordner ins Formular.")
}

/// Zeile mit der Bildschirmgröße im Bestandteil „System“.
pub fn screen_line(width: u32, height: u32) -> String {
    format!("Bildschirm: {width} × {height}")
}

/// Kennung des Fensters (für Lage und Größe in Tests).
pub fn window_id() -> egui::Id {
    egui::Id::new(WINDOW_ID)
}

/// Kennung der aufklappbaren Vorschau von `part`.
pub fn preview_id(part: Part) -> egui::Id {
    egui::Id::new(("planora_feedback_vorschau", part.label()))
}

/// Was beim Öffnen über das Programm bekannt ist (Schnappschuss; ändert sich nicht, solange
/// das Fenster offen ist).
#[derive(Clone, Debug)]
pub struct FeedbackContext {
    pub info: BuildInfo,
    /// Größe des Bildschirms in Pixeln, falls bekannt.
    pub screen: Option<(u32, u32)>,
    pub steps: Vec<Step>,
    /// Die letzten Protokollzeilen (höchstens [`LOG_LINES`] werden genommen).
    pub log: Vec<String>,
    /// Was vor der Anzeige entfernt wird.
    pub personal: Personal,
    /// Datenordner; der Rückmeldungsordner entsteht darunter.
    pub data_dir: PathBuf,
    /// Absturzbericht, der mitgehen kann (Etappe 3).
    pub crash_report: Option<PathBuf>,
    /// Geöffnet aus der Absturz-Rückfrage („Als Rückmeldung öffnen“): Dann ist der
    /// Absturzbericht schon angehakt, weil die Testerin ihn gerade ausdrücklich gewählt hat;
    /// sonst ist er wie Protokoll, Bild und Projektdatei zuerst aus.
    pub from_crash: bool,
}

/// Was die App für das Fenster ausführen soll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedbackAction {
    /// Diese Adresse in einem neuen Browser-Tab öffnen.
    OpenBrowser(String),
    /// Diesen Ordner im Dateimanager öffnen.
    OpenFolder(PathBuf),
    /// Diesen Text in die Zwischenablage legen.
    Copy(String),
    /// Das Fenster wurde geschlossen.
    Close,
}

/// Was „Im Browser öffnen“ getan hat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    /// Die geöffnete Adresse.
    pub url: String,
    /// Der angelegte und geöffnete Rückmeldungsordner.
    pub folder: Option<PathBuf>,
    /// Ein Ordner war nötig, ließ sich aber nicht anlegen ([`NO_FILE`]).
    pub file_failed: bool,
}

/// Ein Bestandteil mit bereinigtem Text und Häkchen.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PartRow {
    part: Part,
    text: String,
    on: bool,
}

/// Wovon die Adresse abhängt (Zwischenspeicher, damit sie nicht jedes Bild neu entsteht).
type AddressKey = (Kind, Option<&'static str>, String, Vec<bool>);

/// Zustand des Fensters zwischen den Bildern (gehört der App).
#[derive(Debug)]
pub struct FeedbackDialog {
    pub open: bool,
    pub kind: Kind,
    /// Einer aus [`AREAS`] oder keiner.
    pub area: Option<&'static str>,
    /// Wie getippt; in der Meldung ohne Leerraum am Rand und nicht bereinigt.
    pub description: String,
    /// Gesetzt nach „Im Browser öffnen“.
    pub sent: Option<Sent>,
    /// „Text kopieren“ wurde geklickt (seit dem Öffnen bzw. Schicken).
    pub copied: bool,
    rows: Vec<PartRow>,
    context: Option<FeedbackContext>,
    focus_description: bool,
    pending: VecDeque<FeedbackAction>,
    address: Option<(AddressKey, Built)>,
    /// Ablauf „Bild vom Fenster“.
    shot: ShotFlow,
}

impl Default for FeedbackDialog {
    fn default() -> Self {
        FeedbackDialog {
            open: false,
            kind: Kind::Fehler,
            area: None,
            description: String::new(),
            sent: None,
            copied: false,
            rows: Vec::new(),
            context: None,
            focus_description: false,
            pending: VecDeque::new(),
            address: None,
            shot: ShotFlow::default(),
        }
    }
}

impl FeedbackDialog {
    /// Öffnet das Fenster leer mit den Angaben `context`; die Bestandteile werden jetzt einmal
    /// bereinigt, die Häkchen stehen wie [`Part::default_on`] (der Absturzbericht ist nur aus der
    /// Absturz-Rückfrage heraus angehakt, [`FeedbackContext::from_crash`]). Das Beschreibungsfeld
    /// bekommt die Tastatur.
    pub fn open_fresh(&mut self, context: FeedbackContext) {
        *self = FeedbackDialog {
            open: true,
            rows: rows_for(&context),
            context: Some(context),
            focus_description: true,
            ..FeedbackDialog::default()
        };
    }

    /// Schließt das Fenster (der Inhalt bleibt bis zum nächsten Öffnen).
    pub fn close(&mut self) {
        self.open = false;
        self.focus_description = false;
    }

    /// Die Bestandteile, die das Fenster zeigt, in ihrer Reihenfolge.
    pub fn shown_parts(&self) -> Vec<Part> {
        self.rows.iter().map(|r| r.part).collect()
    }

    /// Ist das Häkchen von `part` gesetzt? Nicht gezeigte Bestandteile sind nie angehakt.
    pub fn is_checked(&self, part: Part) -> bool {
        self.rows.iter().any(|r| r.part == part && r.on)
    }

    /// Setzt das Häkchen von `part` (nur bei gezeigten Bestandteilen). Beim Bild startet das
    /// Anhaken die Aufnahme (falls noch keins da ist), das Abwählen vergisst es.
    pub fn set_checked(&mut self, part: Part, on: bool) {
        if let Some(r) = self.rows.iter_mut().find(|r| r.part == part) {
            r.on = on;
        }
        if part == Part::Bild {
            self.picture_toggled(on);
        }
    }

    fn picture_toggled(&mut self, on: bool) {
        if on {
            if self.shot.image().is_none() {
                self.shot.start();
                self.set_picture_text(SHOT_NOT_YET.to_string());
            }
        } else {
            self.shot.reset();
            self.set_picture_text(SHOT_NOT_YET.to_string());
        }
    }

    fn set_picture_text(&mut self, text: String) {
        if let Some(r) = self.rows.iter_mut().find(|r| r.part == Part::Bild) {
            r.text = text;
        }
    }

    /// Soll das Fenster gerade für die Aufnahme verborgen sein?
    pub fn hides_for_shot(&self) -> bool {
        self.open && self.shot.hides_dialog()
    }

    /// Einmal je Bild vor dem Zeichnen: liefert, ob die App das Bild anfordern soll. Kam nach
    /// [`SHOT_TIMEOUT`](crate::feedback_shot::SHOT_TIMEOUT) keins, ist das Häkchen wieder aus
    /// und die Vorschau sagt [`SHOT_FAILED`].
    pub fn shot_tick(&mut self, now: Instant) -> ShotStep {
        let step = self.shot.tick(now);
        if step == ShotStep::ShowDialogAgain && matches!(self.shot.phase, ShotPhase::Failed) {
            diag::warn("Bild vom Fenster kam nicht");
            if let Some(r) = self.rows.iter_mut().find(|r| r.part == Part::Bild) {
                r.on = false;
            }
            self.set_picture_text(SHOT_FAILED.to_string());
        }
        step
    }

    /// Ein angefordertes Bild ist da.
    pub fn shot_received(&mut self, img: Arc<egui::ColorImage>) {
        if self.shot.received(img) == ShotStep::ShowDialogAgain
            && let Some(img) = self.shot.image()
        {
            let line = preview_line(img);
            self.set_picture_text(line);
        }
    }

    /// Bereinigter Text von `part`, wie in der Vorschau.
    pub fn preview_text(&self, part: Part) -> Option<&str> {
        self.rows
            .iter()
            .find(|r| r.part == part)
            .map(|r| r.text.as_str())
    }

    /// Ist die Beschreibung lang genug ([`MIN_CHARS`])?
    pub fn long_enough(&self) -> bool {
        self.description.trim().chars().count() >= MIN_CHARS
    }

    /// Die Meldung aus dem aktuellen Stand: nur angehakte Bestandteile.
    pub fn report(&self) -> Report {
        Report {
            kind: self.kind,
            area: self.area.map(str::to_string),
            description: self.description.trim().to_string(),
            parts: self
                .rows
                .iter()
                .filter(|r| r.on)
                .map(|r| (r.part, r.text.clone()))
                .collect(),
        }
    }

    /// Geht etwas mit, das nur als Datei im Ordner Platz hat (Bild, Projektdatei,
    /// Absturzbericht)?
    fn with_file(&self) -> bool {
        self.rows.iter().any(|r| {
            r.on && matches!(
                r.part,
                Part::Bild | Part::Projektdatei | Part::Absturzbericht
            )
        })
    }

    /// Die Adresse aus dem aktuellen Stand ([`feedback::issue_url`]).
    pub fn built(&mut self) -> Built {
        let key: AddressKey = (
            self.kind,
            self.area,
            self.description.clone(),
            self.rows.iter().map(|r| r.on).collect(),
        );
        if let Some((k, built)) = &self.address
            && *k == key
        {
            return built.clone();
        }
        let built = feedback::issue_url(&self.report(), self.with_file());
        // Eine andere Adresse: „Kopiert.“ gilt nicht mehr.
        self.copied = false;
        self.address = Some((key, built.clone()));
        built
    }

    /// Das Bild, wenn es angehakt ist und vorliegt.
    fn picture_to_save(&self) -> Option<Arc<egui::ColorImage>> {
        if self.is_checked(Part::Bild) {
            self.shot.image().cloned()
        } else {
            None
        }
    }

    /// „Im Browser öffnen“: Ist eine Datei nötig ([`Built::needs_file`]), entsteht der
    /// Rückmeldungsordner mit `rueckmeldung.txt` (und `bild.png`, wenn das Bild angehakt ist);
    /// er wird zuerst geöffnet, dann die Adresse. Lässt sich der Ordner nicht anlegen, geht die
    /// Adresse trotzdem (ohne den Hinweis auf die Datei, soweit nichts gekürzt wurde).
    fn send(&mut self) {
        let report = self.report();
        let with_file = self.with_file();
        let mut built = feedback::issue_url(&report, with_file);
        let mut folder = None;
        let mut file_failed = false;
        if built.needs_file {
            let written = match self.context.as_ref() {
                Some(c) => {
                    let dir = feedback::bundle_dir(&c.data_dir, chrono::Local::now().naive_local());
                    feedback::write_bundle(&dir, &report).map(|_| dir)
                }
                None => Err(std::io::Error::other("kein Datenordner")),
            };
            let written = written.and_then(|dir| match self.picture_to_save() {
                Some(img) => save_png(&dir, &img).map(|_| dir),
                None => Ok(dir),
            });
            match written {
                Ok(dir) => folder = Some(dir),
                Err(e) => {
                    file_failed = true;
                    diag::warn(&format!("Rückmeldungsordner nicht angelegt: {e}"));
                    if with_file {
                        built = feedback::issue_url(&report, false);
                    }
                }
            }
        }
        diag::info(&format!(
            "Rückmeldung ({}) im Browser geöffnet",
            report.kind.label()
        ));
        if let Some(dir) = &folder {
            self.pending
                .push_back(FeedbackAction::OpenFolder(dir.clone()));
        }
        self.pending
            .push_back(FeedbackAction::OpenBrowser(built.url.clone()));
        self.copied = false;
        self.focus_description = false;
        self.sent = Some(Sent {
            url: built.url,
            folder,
            file_failed,
        });
    }
}

/// Die Bestandteile aus den Angaben beim Öffnen, bereinigt. Schritte, Protokoll und
/// Absturzbericht nur, wenn es sie gibt.
fn rows_for(c: &FeedbackContext) -> Vec<PartRow> {
    let who = &c.personal;
    let clean = |t: &str| feedback::sanitize(t, who);
    let mut system = c.info.os.clone();
    if let Some((w, h)) = c.screen {
        system.push('\n');
        system.push_str(&screen_line(w, h));
    }
    let mut rows = vec![
        (Part::Version, clean(&c.info.version_line())),
        (Part::System, clean(&system)),
        (Part::Grafik, feedback::gpu_line(&c.info, who)),
    ];
    if !c.steps.is_empty() {
        rows.push((Part::Schritte, clean(&diag::steps_text(&c.steps))));
    }
    if !c.log.is_empty() {
        let from = c.log.len().saturating_sub(LOG_LINES);
        rows.push((Part::Protokoll, clean(&c.log[from..].join("\n"))));
    }
    rows.push((Part::Bild, SHOT_NOT_YET.to_string()));
    if let Some(text) = c.crash_report.as_deref().and_then(read_report) {
        rows.push((Part::Absturzbericht, clean(&text)));
    }
    rows.into_iter()
        .map(|(part, text)| PartRow {
            part,
            on: part.default_on() || (part == Part::Absturzbericht && c.from_crash),
            text,
        })
        .collect()
}

fn read_report(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Esc schließt das offene Fenster und wird verbraucht (vor den Ansichten aufrufen). Nicht bei
/// offener Rückfrage und nicht, solange ein Textfeld die Tastatur hat (dann gibt das Feld sie
/// mit Esc ab; das nächste Esc schließt).
pub fn take_escape(ctx: &egui::Context, state: &mut FeedbackDialog, prompt_open: bool) -> bool {
    if !state.open || prompt_open || crate::viewport::typing(ctx) {
        return false;
    }
    let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if esc {
        state.close();
    }
    esc
}

/// Klick in der Fußzeile.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FooterClick {
    Send,
    Cancel,
}

/// Zeichnet das Fenster, solange `state.open` gilt, und liefert, was die App ausführen soll
/// (je Bild eine Aktion; „Im Browser öffnen“ liefert Ordner und Adresse nacheinander).
pub fn show(ctx: &egui::Context, state: &mut FeedbackDialog, dark: bool) -> Option<FeedbackAction> {
    if !state.open {
        return state.pending.pop_front();
    }
    let mut open = true;
    let mut action = None;
    let mut click = None;
    let sent = state.sent.is_some();
    let enough = Cell::new(state.long_enough());
    Dialog::new(TITLE, DialogSize::Wide)
        .id(window_id())
        .center_on_open()
        .open(&mut open)
        .show_with_footer(
            ctx,
            |ui| {
                if sent {
                    after_send(ui, state, dark, &mut action);
                } else {
                    form(ui, state, &mut action);
                    enough.set(state.long_enough());
                }
            },
            |ui| footer(ui, sent, enough.get(), dark, &mut click),
        );
    match click {
        Some(FooterClick::Send) if !sent && state.long_enough() => state.send(),
        Some(FooterClick::Cancel) => open = false,
        _ => {}
    }
    if !open {
        state.close();
        action = Some(FeedbackAction::Close);
    }
    if let Some(a) = action {
        state.pending.push_front(a);
    }
    let next = state.pending.pop_front();
    if !state.pending.is_empty() {
        ctx.request_repaint();
    }
    next
}

/// Art, Bereich, Beschreibung, Bestandteile mit Vorschau und die Adresse.
fn form(ui: &mut egui::Ui, state: &mut FeedbackDialog, action: &mut Option<FeedbackAction>) {
    ui.horizontal(|ui| {
        ui.strong(KIND_LABEL);
        ui.add_space(SPACE_2);
        for k in [Kind::Fehler, Kind::Wunsch, Kind::Frage] {
            let r = ui.radio_value(&mut state.kind, k, k.label());
            theme::focus_ring(ui, &r);
        }
    });
    ui.add_space(SPACE_2);
    ui.horizontal(|ui| {
        ui.strong(AREA_LABEL);
        ui.add_space(SPACE_2);
        egui::ComboBox::from_id_salt("planora_feedback_bereich")
            .selected_text(state.area.unwrap_or(AREA_NONE))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.area, None, AREA_NONE);
                for a in AREAS {
                    ui.selectable_value(&mut state.area, Some(a), a);
                }
            });
    });
    ui.add_space(SPACE_3);
    ui.strong(DESCRIPTION_LABEL);
    ui.add_space(SPACE_1);
    let r = theme::field(
        ui,
        egui::TextEdit::multiline(&mut state.description)
            .desired_rows(DESCRIPTION_ROWS)
            .desired_width(f32::INFINITY),
    );
    if state.focus_description {
        r.request_focus();
        state.focus_description = false;
    }

    ui.add_space(SPACE_4);
    theme::section_header(ui, PREVIEW_HEADING, theme::SUB_HEADER_H);
    ui.add_space(SPACE_1);
    let picture_before = state.is_checked(Part::Bild);
    for row in &mut state.rows {
        egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            preview_id(row.part),
            false,
        )
        .show_header(ui, |ui| {
            let r = ui.checkbox(&mut row.on, row.part.label());
            theme::focus_ring(ui, &r);
        })
        .body(|ui| read_only_text(ui, preview_id(row.part), &row.text, PREVIEW_MAX_H));
    }
    let picture_now = state.is_checked(Part::Bild);
    if picture_now != picture_before {
        state.picture_toggled(picture_now);
    }

    ui.add_space(SPACE_3);
    let url = state.built().url;
    egui::CollapsingHeader::new(SHOW_ADDRESS)
        .id_salt("planora_feedback_adresse")
        .default_open(false)
        .show(ui, |ui| {
            read_only_text(
                ui,
                egui::Id::new("planora_feedback_adresse_text"),
                &url,
                ADDRESS_MAX_H,
            );
            ui.add_space(SPACE_1);
            copy_row(ui, state, &url, action);
        });
}

/// Nach „Im Browser öffnen“: Hinweis mit „Text kopieren“, Ordner-Hinweis oder [`NO_FILE`],
/// die geöffnete Adresse zum Aufklappen.
fn after_send(
    ui: &mut egui::Ui,
    state: &mut FeedbackDialog,
    dark: bool,
    action: &mut Option<FeedbackAction>,
) {
    let Some(sent) = state.sent.clone() else {
        return;
    };
    ui.add(egui::Label::new(BROWSER_HINT).wrap());
    ui.add_space(SPACE_1);
    copy_row(ui, state, &sent.url, action);
    if sent.file_failed {
        ui.add_space(SPACE_3);
        crate::update_dialog::failure_box(ui, NO_FILE, dark);
    } else if sent.folder.is_some() {
        ui.add_space(SPACE_3);
        ui.add(egui::Label::new(folder_hint(FEEDBACK_FILE)).wrap());
    }
    ui.add_space(SPACE_3);
    egui::CollapsingHeader::new(SHOW_ADDRESS)
        .id_salt("planora_feedback_adresse_geschickt")
        .default_open(false)
        .show(ui, |ui| {
            read_only_text(
                ui,
                egui::Id::new("planora_feedback_adresse_geschickt_text"),
                &sent.url,
                ADDRESS_MAX_H,
            );
        });
}

/// „Text kopieren“ und danach „Kopiert.“.
fn copy_row(
    ui: &mut egui::Ui,
    state: &mut FeedbackDialog,
    url: &str,
    action: &mut Option<FeedbackAction>,
) {
    ui.horizontal(|ui| {
        let r = ui.button(COPY_TEXT);
        theme::focus_ring(ui, &r);
        if r.clicked() {
            state.copied = true;
            *action = Some(FeedbackAction::Copy(url.to_string()));
        }
        if state.copied {
            ui.label(COPIED);
        }
    });
}

/// Schreibgeschützter Mehrzeilentext (markier- und kopierbar), höchstens `max_h` hoch.
fn read_only_text(ui: &mut egui::Ui, salt: egui::Id, text: &str, max_h: f32) {
    theme::scroll_area(
        ui,
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .max_height(max_h)
            .auto_shrink([false, true]),
        |ui| {
            let mut shown = text;
            theme::field(
                ui,
                egui::TextEdit::multiline(&mut shown)
                    .desired_rows(1)
                    .desired_width(f32::INFINITY),
            );
        },
    );
}

/// Datenschutz-Satz und Knöpfe: vor dem Schicken „Im Browser öffnen“ (gesperrt mit Hinweis,
/// solange die Beschreibung zu kurz ist) und „Abbrechen“, danach „Schließen“.
fn footer(
    ui: &mut egui::Ui,
    sent: bool,
    enough: bool,
    dark: bool,
    click: &mut Option<FooterClick>,
) {
    ui.add_space(SPACE_2);
    ui.label(egui::RichText::new(PRIVACY_LINE).color(theme::palette(dark).text_secondary));
    ui.add_space(SPACE_2);
    ui.horizontal(|ui| {
        if sent {
            let r = ui.add(theme::primary_button(CLOSE, dark));
            theme::focus_ring(ui, &r);
            if r.clicked() {
                *click = Some(FooterClick::Cancel);
            }
            return;
        }
        let r = ui
            .add_enabled(enough, theme::primary_button(OPEN_BROWSER, dark))
            .on_disabled_hover_text(TOO_SHORT);
        theme::focus_ring(ui, &r);
        if r.clicked() {
            *click = Some(FooterClick::Send);
        }
        let r = ui.button(CANCEL);
        theme::focus_ring(ui, &r);
        if r.clicked() {
            *click = Some(FooterClick::Cancel);
        }
        if !enough {
            ui.add_space(SPACE_2);
            ui.label(egui::RichText::new(TOO_SHORT).color(theme::palette(dark).text_secondary));
        }
    });
}

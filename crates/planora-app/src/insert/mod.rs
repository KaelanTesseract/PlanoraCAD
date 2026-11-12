//! Menü „Einfügen“ (Skizzen 9, Spec C): DXF, SVG und Leinwand. Hier das gemeinsame Gerüst der
//! Fenster: Lesen im Hintergrund mit Laufnummer ([`BackgroundRead`]), die Antwort der Fenster
//! ([`DialogAction`]), die Einheiten der Datei und der Hinweis zum Übersprungenen.

pub mod dxf_dialog;
pub mod pick;
pub mod svg_dialog;

use crate::jobs::{Job, Poll};
use crate::msg::{Msg, MsgArg};
use planora_exchange::import2d::{Import2d, Skipped, Unit2d};
use planora_exchange::{
    ExchangeError, MAX_DXF_BYTES, MAX_SVG_BYTES, Progress, READ_BUFFER, read_dxf, read_svg,
};
use planora_i18n::{DisplayUnit, tr};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Antwort eines Einfügen-Fensters nach einem Bild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAction {
    /// Das Fenster bleibt offen.
    Open,
    /// Geschlossen ohne Änderung (Abbrechen, Esc, ✕).
    Closed,
    /// Eingefügt (ein Rückgängig-Schritt), das Fenster ist zu.
    Inserted,
}

/// Leser einer Datei für [`BackgroundRead`]: Inhalt und Abbruchfrage.
pub type Reader<T> = fn(&[u8], &dyn Fn() -> bool) -> Result<T, ExchangeError>;

/// Ein Leser mit seiner Größengrenze: Eine größere Datei wird gar nicht erst gelesen, der Fehler
/// ist dann `too_large`. Die Grenze gehört zum Leser (DXF und SVG je 256 MiB, Bilder 64 MiB).
#[derive(Clone, Debug)]
pub struct ReadSpec<T> {
    pub max_bytes: u64,
    pub too_large: ExchangeError,
    pub read: Reader<T>,
}

/// Der DXF-Leser mit [`MAX_DXF_BYTES`].
pub fn dxf_spec() -> ReadSpec<Import2d> {
    ReadSpec {
        max_bytes: MAX_DXF_BYTES,
        too_large: ExchangeError::DxfTooLarge,
        read: read_dxf,
    }
}

/// Der SVG-Leser mit [`MAX_SVG_BYTES`].
pub fn svg_spec() -> ReadSpec<Import2d> {
    ReadSpec {
        max_bytes: MAX_SVG_BYTES,
        too_large: ExchangeError::SvgTooLarge,
        read: read_svg,
    }
}

/// Nummer des letzten gestarteten Laufs (über alle Fenster).
static RUNS: AtomicU64 = AtomicU64::new(0);

/// Lesen einer Datei im Arbeitsthread mit Fortschritt und Abbrechen. Jeder Lauf hat eine eigene
/// Nummer; ein Ergebnis mit fremder Nummer wird verworfen (ein abgebrochener oder ersetzter Lauf
/// erreicht nie ein Fenster).
///
/// Die Nummer ist eine Absicherung: Jeder `BackgroundRead` hat ohnehin seinen eigenen Kanal,
/// fremde Ergebnisse kommen dort heute gar nicht an. Sie bleibt, falls ein Lauf je einen Kanal
/// mit einem anderen teilt.
pub struct BackgroundRead<T> {
    run: u64,
    path: PathBuf,
    job: Option<Job<(u64, Result<T, ExchangeError>)>>,
}

impl<T: Send + 'static> BackgroundRead<T> {
    /// Startet das Lesen von `path` mit dem Leser `spec`.
    pub fn start(path: PathBuf, spec: ReadSpec<T>) -> Self {
        let run = RUNS.fetch_add(1, Ordering::Relaxed) + 1;
        let source = path.clone();
        let job = Job::spawn(move |progress| (run, read_with(&source, progress, &spec)));
        BackgroundRead {
            run,
            path,
            job: Some(job),
        }
    }
}

impl<T> BackgroundRead<T> {
    /// Das Ergebnis, sobald es da ist (genau einmal); sonst `None`. Nach [`Self::cancel`] immer
    /// `None`.
    pub fn poll(&mut self) -> Option<Result<T, ExchangeError>> {
        self.fetch(false)
    }

    /// Wie [`Self::poll`], wartet aber auf das Ergebnis (Tests und Abläufe ohne Fenster).
    pub fn wait(&mut self) -> Option<Result<T, ExchangeError>> {
        self.fetch(true)
    }

    fn fetch(&mut self, block: bool) -> Option<Result<T, ExchangeError>> {
        let polled = self.job.as_ref()?.fetch(block);
        match polled {
            Poll::Pending => None,
            Poll::Ready((run, result)) => {
                self.job = None;
                // Ein Ergebnis eines anderen Laufs gehört nicht hierher.
                (run == self.run).then_some(result)
            }
            Poll::Lost => {
                self.job = None;
                Some(Err(ExchangeError::Io {
                    msg: tr!("insert-read-lost"),
                }))
            }
        }
    }

    /// Bricht ab: der Arbeitsthread endet beim nächsten Prüfen, sein Ergebnis kommt nie an.
    pub fn cancel(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel();
        }
    }

    /// Läuft das Lesen noch?
    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    /// Erledigter Anteil zwischen 0 und 1.
    pub fn progress(&self) -> f32 {
        self.job.as_ref().map_or(0.0, Job::fraction)
    }

    /// Nummer dieses Laufs.
    pub fn run(&self) -> u64 {
        self.run
    }

    /// Die gelesene Datei.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Liest die Datei stückweise (erste Hälfte des Fortschritts) und gibt sie dem Leser (zweite
/// Hälfte). Eine Datei über der Grenze des Lesers wird vorher abgelehnt.
fn read_with<T>(path: &Path, progress: &Progress, spec: &ReadSpec<T>) -> Result<T, ExchangeError> {
    let io = |e: std::io::Error| ExchangeError::Io { msg: e.to_string() };
    let mut file = std::fs::File::open(path).map_err(io)?;
    let len = file.metadata().map_err(io)?.len();
    if len > spec.max_bytes {
        return Err(spec.too_large.clone());
    }
    progress.set_total(len.saturating_mul(2).max(1));
    let mut bytes = Vec::with_capacity(len as usize);
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        progress.check()?;
        let n = file.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        // Wächst die Datei beim Lesen, gilt die Grenze trotzdem.
        if bytes.len() as u64 > spec.max_bytes {
            return Err(spec.too_large.clone());
        }
        progress.advance(n as u64);
    }
    let result = (spec.read)(&bytes, &|| progress.is_cancelled())?;
    progress.check()?;
    progress.advance(len.max(1));
    Ok(result)
}

/// Die Einheiten der Liste in ihrer Reihenfolge (mm, cm, m, in, ft).
pub const UNITS: [Unit2d; 5] = [
    Unit2d::Millimeter,
    Unit2d::Centimeter,
    Unit2d::Meter,
    Unit2d::Inch,
    Unit2d::Foot,
];

/// Name einer Einheit in der Liste.
pub fn unit_label(u: Unit2d) -> String {
    match u {
        Unit2d::Millimeter => tr!("import-unit-mm"),
        Unit2d::Centimeter => tr!("import-unit-cm"),
        Unit2d::Meter => tr!("import-unit-m"),
        Unit2d::Inch => tr!("import-unit-inch"),
        Unit2d::Foot => tr!("insert-unit-ft"),
    }
}

/// Vorwahl ohne Einheit in der Datei: die Anzeigeeinheit der Einstellungen (Spec, „Nach der
/// Prüfung festgelegt“).
pub fn display_unit() -> Unit2d {
    match planora_i18n::current_unit() {
        DisplayUnit::Millimeter => Unit2d::Millimeter,
        DisplayUnit::Zoll => Unit2d::Inch,
    }
}

/// Hinweis zum Übersprungenen („12 Bemaßungen und 3 Schraffuren übersprungen.“), nur mit den
/// Arten, von denen es etwas gibt; `None`, wenn nichts übersprungen wurde.
pub fn skipped_summary(s: &Skipped) -> Option<Msg> {
    let kinds = [
        (s.dimensions, "insert-skipped-dimensions"),
        (s.hatches, "insert-skipped-hatches"),
        (s.texts, "insert-skipped-texts"),
        (s.images, "insert-skipped-images"),
        (s.three_d, "insert-skipped-three-d"),
        (s.unknown, "insert-skipped-unknown"),
        (s.degenerate, "insert-skipped-degenerate"),
    ];
    let items: Vec<Msg> = kinds
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, id)| Msg::Id(id, vec![("n", MsgArg::from(n))]))
        .collect();
    let items = join_and(items)?;
    Some(crate::msg!("insert-skipped-summary", items = items))
}

/// „a, b und c“: alle bis auf das letzte mit Komma, das letzte mit „und“.
fn join_and(mut items: Vec<Msg>) -> Option<Msg> {
    let last = items.pop()?;
    if items.is_empty() {
        return Some(last);
    }
    let first = MsgArg::List(items.into_iter().map(MsgArg::from).collect());
    Some(Msg::Id(
        "insert-list-and",
        vec![("first", first), ("last", MsgArg::from(last))],
    ))
}

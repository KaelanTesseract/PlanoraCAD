//! Prüfung auf neue Versionen im Hintergrund: ein eigener Thread fragt die Quelle
//! ([`planora_update::check::ReleaseSource`]) und schickt das Ergebnis über einen Kanal zurück;
//! die Oberfläche fragt es in jedem Bild ab, ohne zu warten ([`UpdateChecker::poll`]). Ist ein
//! egui-Kontext hinterlegt, fordert der Thread nach getaner Arbeit ein neues Bild an.
//!
//! Was die App mit dem Ergebnis macht (Zeitpunkt merken, Hinweis in den Einstellungen,
//! vorgemerkte Meldung), steht in [`crate::app`]; hier nur der Ablauf und die Regeln dazu.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use planora_i18n::tr;
use planora_update::check::{CheckOutcome, Checker, NetError, ReleaseSource};
use planora_update::{Channel, Version};

/// Was geprüft werden soll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckRequest {
    /// Laufende Version.
    pub running: Version,
    /// Update-Kanal des Nutzers.
    pub channel: Channel,
    /// Übersprungene Version (wird nicht angeboten); bei der Suche von Hand keine.
    pub skipped: Option<Version>,
    /// Vom Knopf „Jetzt nach Updates suchen“ (dann gibt es auch ohne Update einen Hinweis).
    pub manual: bool,
}

/// Ergebnis einer Prüfung.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    pub outcome: CheckOutcome,
    /// Wie [`CheckRequest::manual`].
    pub manual: bool,
    /// Ende der Prüfung (Sekunden seit 1970, UTC).
    pub at: u64,
}

/// Läuft höchstens eine Prüfung zur Zeit.
#[derive(Default)]
pub struct UpdateChecker {
    handle: Option<JoinHandle<()>>,
    rx: Option<Receiver<CheckResult>>,
    /// `manual` der laufenden Prüfung (falls der Thread ohne Ergebnis endet).
    manual: bool,
    /// Kontext, dem der Thread nach getaner Arbeit ein neues Bild meldet.
    repaint: Option<egui::Context>,
}

impl UpdateChecker {
    /// Hinterlegt den Kontext, dem eine fertige Prüfung ein neues Bild meldet.
    pub fn set_repaint(&mut self, ctx: &egui::Context) {
        self.repaint = Some(ctx.clone());
    }

    /// Läuft gerade eine Prüfung (Ergebnis noch nicht abgeholt)?
    pub fn is_running(&self) -> bool {
        self.rx.is_some()
    }

    /// Startet eine Prüfung in einem eigenen Thread. Läuft schon eine, bleibt es bei ihr.
    pub fn start(&mut self, source: Arc<dyn ReleaseSource + Send + Sync>, request: CheckRequest) {
        if self.is_running() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let repaint = self.repaint.clone();
        let manual = request.manual;
        let spawned = std::thread::Builder::new()
            .name("planora-update-pruefung".into())
            .spawn(crate::update_install::in_lang(move || {
                let outcome = Checker::new(source).check(
                    &request.running,
                    request.channel,
                    request.skipped.as_ref(),
                );
                // Ist die Oberfläche schon weg, ist das Ergebnis egal.
                let _ = tx.send(CheckResult {
                    outcome,
                    manual,
                    at: now_unix(),
                });
                if let Some(ctx) = repaint {
                    ctx.request_repaint();
                }
            }));
        self.manual = manual;
        match spawned {
            Ok(handle) => {
                self.handle = Some(handle);
                self.rx = Some(rx);
            }
            // Kein Thread zu haben: wie ein Fehler der Suche, ohne Netz gefragt zu haben.
            Err(e) => {
                let (tx, rx) = mpsc::channel();
                let _ = tx.send(CheckResult {
                    outcome: CheckOutcome::Failed(NetError::Other(e.to_string())),
                    manual,
                    at: now_unix(),
                });
                self.rx = Some(rx);
            }
        }
    }

    /// Holt das Ergebnis ab, falls fertig; wartet nie. Endet der Thread ohne Ergebnis, kommt ein
    /// Fehler zurück.
    pub fn poll(&mut self) -> Option<CheckResult> {
        let result = match self.rx.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => CheckResult {
                outcome: CheckOutcome::Failed(NetError::Other(tr!("update-detail-check-aborted"))),
                manual: self.manual,
                at: now_unix(),
            },
        };
        self.rx = None;
        // Der Thread hat gesendet und endet gleich; nicht auf ihn warten.
        self.handle = None;
        Some(result)
    }
}

/// Wird der Zeitpunkt der Prüfung gemerkt? Nach jeder Prüfung, außer wenn kein Netz da war
/// oder die Zeit ablief: dann wird es beim nächsten Start erneut versucht.
pub fn records_check(outcome: &CheckOutcome) -> bool {
    !matches!(
        outcome,
        CheckOutcome::Failed(NetError::Offline | NetError::Timeout)
    )
}

/// Jetzt in Sekunden seit 1970 (UTC); eine Uhr vor 1970 gilt als 0.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Quelle ohne Netz für [`crate::app::PlanoraApp::for_tests`]: meldet immer „kein Netz“ (dann
/// wird auch nichts gespeichert).
pub struct NoNetwork;

impl ReleaseSource for NoNetwork {
    fn fetch_releases(&self) -> Result<String, NetError> {
        Err(NetError::Offline)
    }
}

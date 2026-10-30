//! Bild vom Fenster für eine Rückmeldung: das Fenster „Rückmeldung“ ein Bild lang verbergen,
//! über [`egui::ViewportCommand::Screenshot`] ein Bild anfordern, das Ergebnis behalten und das
//! Fenster wieder zeigen. Das Bild bleibt im Speicher, bis die Testerin auf „Im Browser öffnen“
//! klickt; erst dann landet es als [`SHOT_FILE`] im Rückmeldungsordner.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

/// Dateiname des Bildes im Rückmeldungsordner.
pub const SHOT_FILE: &str = "bild.png";
/// So lange wird auf das Bild gewartet.
pub const SHOT_TIMEOUT: Duration = Duration::from_secs(3);
/// Hinweis, wenn kein Bild kam oder es sich nicht speichern ließ.
pub fn shot_failed() -> String {
    planora_i18n::tr!("feedback-shot-failed")
}
/// Kennung an der Bildanforderung, damit fremde Bilder nicht verwechselt werden.
pub const FEEDBACK_SHOT_TAG: &str = "planora_feedback_bild";

/// Kodiert `img` als PNG (RGBA, 8 Bit).
pub fn encode_png(img: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = img.size;
    let mut data = Vec::with_capacity(w * h * 4);
    for px in &img.pixels {
        data.extend_from_slice(&px.to_srgba_unmultiplied());
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&data).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Speichert `img` als `dir/bild.png` (der Ordner wird angelegt).
pub fn save_png(dir: &Path, img: &egui::ColorImage) -> std::io::Result<PathBuf> {
    let bytes = encode_png(img).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(dir)?;
    let path = dir.join(SHOT_FILE);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Wo der Ablauf steht.
#[derive(Clone, Debug, Default)]
pub enum ShotPhase {
    #[default]
    Idle,
    /// Das Fenster „Rückmeldung“ ist ein Bild lang verborgen.
    Hiding,
    /// Das Bild ist angefordert.
    Waiting { since: Instant },
    /// Das Bild ist da.
    Done(Arc<egui::ColorImage>),
    /// Es kam kein Bild ([`SHOT_TIMEOUT`]).
    Failed,
}

/// Was die App in diesem Bild tun soll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotStep {
    Nothing,
    /// [`egui::ViewportCommand::Screenshot`] mit [`shot_user_data`] schicken.
    RequestScreenshot,
    /// Das Fenster „Rückmeldung“ wieder zeigen.
    ShowDialogAgain,
}

/// Ablauf „Bild vom Fenster“.
#[derive(Clone, Debug, Default)]
pub struct ShotFlow {
    pub phase: ShotPhase,
}

impl ShotFlow {
    /// Startet den Ablauf (ein laufender Ablauf beginnt neu).
    pub fn start(&mut self) {
        self.phase = ShotPhase::Hiding;
    }

    /// Soll das Fenster „Rückmeldung“ gerade verborgen sein?
    pub fn hides_dialog(&self) -> bool {
        matches!(self.phase, ShotPhase::Hiding | ShotPhase::Waiting { .. })
    }

    /// Einmal je Bild aufrufen, bevor das Fenster gezeichnet wird.
    pub fn tick(&mut self, now: Instant) -> ShotStep {
        match self.phase {
            ShotPhase::Hiding => {
                // Das Bild davor war ohne Fenster: jetzt anfordern.
                self.phase = ShotPhase::Waiting { since: now };
                ShotStep::RequestScreenshot
            }
            ShotPhase::Waiting { since } if now.duration_since(since) >= SHOT_TIMEOUT => {
                self.phase = ShotPhase::Failed;
                ShotStep::ShowDialogAgain
            }
            _ => ShotStep::Nothing,
        }
    }

    /// Ein Bild ist eingetroffen. Nur angenommen, solange darauf gewartet wird.
    pub fn received(&mut self, img: Arc<egui::ColorImage>) -> ShotStep {
        if matches!(self.phase, ShotPhase::Waiting { .. }) {
            self.phase = ShotPhase::Done(img);
            ShotStep::ShowDialogAgain
        } else {
            ShotStep::Nothing
        }
    }

    /// Das Bild, falls vorhanden.
    pub fn image(&self) -> Option<&Arc<egui::ColorImage>> {
        match &self.phase {
            ShotPhase::Done(img) => Some(img),
            _ => None,
        }
    }

    /// Zurück auf Anfang (Häkchen abgewählt, Fenster neu geöffnet).
    pub fn reset(&mut self) {
        self.phase = ShotPhase::Idle;
    }
}

/// Die Kennung für die Bildanforderung.
pub fn shot_user_data() -> egui::UserData {
    egui::UserData::new(FEEDBACK_SHOT_TAG)
}

/// Gehört das Bild-Ereignis mit `data` zu dieser Rückmeldung?
pub fn is_our_shot(data: &egui::UserData) -> bool {
    data.data
        .as_ref()
        .and_then(|d| d.downcast_ref::<&str>())
        .is_some_and(|tag| *tag == FEEDBACK_SHOT_TAG)
}

/// Text der Vorschau zum Bild.
pub fn preview_line(img: &egui::ColorImage) -> String {
    let [w, h] = img.size;
    format!("{SHOT_FILE}, {w} × {h} pixels")
}

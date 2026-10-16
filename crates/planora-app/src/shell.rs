//! Zustand und Geometrie der schwebenden Felder (reine Funktionen, kein egui-Zeichnen).

use egui::{Rect, pos2, vec2};

use crate::theme;

/// Außenrand der Felder zum Fensterrand.
pub const SHELL_INSET: f32 = theme::SPACE_4;
/// Höhe des oberen Feldes.
pub const TOP_H: f32 = theme::TOOLBAR_H;
/// Höhe des unteren Befehlsfeldes.
pub const BOTTOM_H: f32 = theme::DOCK_H;
/// Abstand zwischen zwei Feldern.
pub const GAP: f32 = theme::SPACE_3;
/// Höhe des Statusfeldes unten links.
pub const STATUS_H: f32 = 32.0;
/// Höhe des schmalen Hinweisfeldes unter dem oberen Feld.
pub const BANNER_H: f32 = 28.0;
/// Untergrenze der verfügbaren Breite für das obere und untere Feld (schmale Fenster).
pub const MIN_FIELD_W: f32 = 120.0;
/// Mindestkante der freien Fläche.
pub const MIN_FREE: f32 = 200.0;
/// Schmalste Seitenspalte, wenn das Fenster zu klein wird.
pub const MIN_SIDE_W: f32 = 160.0;

/// Reiter im rechten Feld.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RightTab {
    Properties,
    Versions,
}

/// Sichtbarkeit und Reiter der Felder.
#[derive(Clone, Debug)]
pub struct ShellState {
    pub projects_visible: bool,
    pub versions_visible: bool,
    pub right_tab: RightTab,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            projects_visible: true,
            versions_visible: true,
            right_tab: RightTab::Properties,
        }
    }
}

impl ShellState {
    /// Projekte einblenden (zum Beispiel nach „Neues Projekt“).
    pub fn show_projects(&mut self) {
        self.projects_visible = true;
    }

    /// Versionen ein- oder ausblenden; ohne Versionen zeigt die rechte Spalte wieder Eigenschaften.
    pub fn set_versions_visible(&mut self, v: bool) {
        self.versions_visible = v;
        if !v {
            self.right_tab = RightTab::Properties;
        }
    }

    /// Der tatsächlich gezeigte Reiter der rechten Spalte.
    pub fn effective_right_tab(&self) -> RightTab {
        if self.versions_visible {
            self.right_tab
        } else {
            RightTab::Properties
        }
    }
}

/// Rechtecke aller Felder für ein Fenster (`content`): oben und unten mittig mit den
/// gewünschten Breiten, links und rechts je eine Spalte, Status links unten, Hinweis
/// unter dem oberen Feld (nur schreibgeschützt) und die freie Fläche für Modell/Blatt.
#[derive(Clone, Copy, Debug)]
pub struct ShellRects {
    pub top: Rect,
    pub left: Rect,
    pub right: Rect,
    pub bottom: Rect,
    pub status: Rect,
    pub banner: Option<Rect>,
    pub free: Rect,
}

/// Berechnet die Geometrie deterministisch aus Konstanten (kein Messen, keine Rückkopplung).
pub fn layout(
    content: Rect,
    _state: &ShellState,
    top_width: f32,
    bottom_width: f32,
    status_width: f32,
    read_only: bool,
) -> ShellRects {
    // Die Seitenspalten sind immer vorhanden (Baum links, Eigenschaften rechts); die
    // Umschalter für Projekte und Versionen ändern nur ihren Inhalt, nicht die Geometrie.
    let normal_free_w = content.width() - 2.0 * (SHELL_INSET + theme::PANEL_WIDTH + GAP);
    let banner_extra = if read_only { BANNER_H + GAP } else { 0.0 };
    let normal_free_h =
        content.height() - (2.0 * SHELL_INSET + TOP_H + BOTTOM_H + 2.0 * GAP + banner_extra);
    // Zu kleines Fenster: Seitenspalten schmaler, damit die freie Fläche 200 breit bleibt.
    let side_w = if normal_free_w < MIN_FREE {
        MIN_SIDE_W.max((content.width() - MIN_FREE - 4.0 * GAP) / 2.0)
    } else {
        theme::PANEL_WIDTH
    };

    // Freie Fläche aus den Rändern; unter 200 wird sie mittig im Fenster auf 200 gesetzt.
    let mut free = Rect::from_min_max(
        pos2(
            content.left() + SHELL_INSET + side_w + GAP,
            content.top() + SHELL_INSET + TOP_H + GAP + banner_extra,
        ),
        pos2(
            content.right() - (SHELL_INSET + side_w + GAP),
            content.bottom() - (SHELL_INSET + BOTTOM_H + GAP),
        ),
    );
    if normal_free_w < MIN_FREE {
        free = Rect::from_center_size(
            pos2(content.center().x, free.center().y),
            vec2(MIN_FREE, free.height()),
        );
    }
    if normal_free_h < MIN_FREE {
        free = Rect::from_center_size(
            pos2(free.center().x, content.center().y),
            vec2(free.width(), MIN_FREE),
        );
    }

    // Mittlere Felder: höchstens so breit, wie zwischen den Spalten Platz ist (mindestens 120,
    // aber nie breiter als das Fenster abzüglich der Außenränder).
    let avail = content.width() - 2.0 * (side_w + SHELL_INSET + GAP);
    let max_w = content.width() - 2.0 * SHELL_INSET;
    let mid_w = |wanted: f32| wanted.min(avail.max(MIN_FIELD_W)).min(max_w);

    let top = Rect::from_center_size(
        pos2(
            content.center().x,
            content.top() + SHELL_INSET + TOP_H / 2.0,
        ),
        vec2(mid_w(top_width), TOP_H),
    );
    let bottom = Rect::from_center_size(
        pos2(
            content.center().x,
            content.bottom() - SHELL_INSET - BOTTOM_H / 2.0,
        ),
        vec2(mid_w(bottom_width), BOTTOM_H),
    );
    let status = Rect::from_min_size(
        pos2(
            content.left() + SHELL_INSET,
            content.bottom() - SHELL_INSET - STATUS_H,
        ),
        vec2(status_width.min(side_w), STATUS_H),
    );

    let col_top = content.top() + SHELL_INSET;
    // Das linke Feld endet mit der freien Fläche, auf jeden Fall um GAP über dem Status.
    let left_bottom = free.bottom().min(status.top() - GAP).max(col_top);
    let right_bottom = free.bottom().max(col_top);
    let left = Rect::from_min_max(
        pos2(content.left() + SHELL_INSET, col_top),
        pos2(content.left() + SHELL_INSET + side_w, left_bottom),
    );
    let right = Rect::from_min_max(
        pos2(content.right() - SHELL_INSET - side_w, col_top),
        pos2(content.right() - SHELL_INSET, right_bottom),
    );

    // Hinweis: gleiche Breite wie das obere Feld, damit er die Spalten nie berührt.
    let banner = read_only.then(|| {
        Rect::from_min_size(
            pos2(top.left(), top.bottom() + GAP),
            vec2(top.width(), BANNER_H),
        )
    });

    ShellRects {
        top,
        left,
        right,
        bottom,
        status,
        banner,
        free,
    }
}

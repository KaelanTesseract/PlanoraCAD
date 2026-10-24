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
/// Höhe des Statusfeldes unten links (eine Zeile; bei langem Text wächst es auf bis zu drei Zeilen
/// (`STATUS_ROWS`) nach oben, bleibt dabei unter der freien Fläche).
pub const STATUS_H: f32 = 32.0;
/// Platz für das Hinweisfeld unter dem oberen Feld: bis zu zwei Zeilen. Eine Zeile braucht nur
/// [`BANNER_LINE_H`].
pub const BANNER_H: f32 = 40.0;
/// Höhe des Hinweisfeldes mit einer Zeile.
pub const BANNER_LINE_H: f32 = 28.0;
/// Untergrenze der verfügbaren Breite für das obere und untere Feld (schmale Fenster).
pub const MIN_FIELD_W: f32 = 120.0;
/// Mindestkante der freien Fläche.
pub const MIN_FREE: f32 = 200.0;
/// Schmalste Seitenspalte, wenn das Fenster zu klein wird.
pub const MIN_SIDE_W: f32 = 160.0;
/// Breite der senkrechten Werkzeugleiste im Skizzenmodus.
pub const STRIP_W: f32 = 44.0;
/// Größe eines Werkzeugknopfs der Leiste (quadratisch).
pub const STRIP_BUTTON: f32 = 36.0;
/// Abstand zwischen zwei Werkzeugknöpfen der Leiste.
pub const STRIP_SPACING: f32 = 4.0;
/// Rand der Leiste: 1 px Strich und Innenabstand, so dass ein Knopf (36) in 44 passt.
pub const STRIP_PAD: f32 = (STRIP_W - STRIP_BUTTON) / 2.0;
/// Höhe des Werkzeugfelds der Zeichnung: zwei feste Zeilen (Ansichten und Blatt, Maße).
pub const TOOL_H: f32 = 76.0;
/// Höhe der Verlaufsleiste über dem Befehlsfeld (nur im Modell).
pub const HISTORY_H: f32 = theme::TOOLBAR_H;
/// Anzahl der Werkzeuge in der Leiste.
pub const STRIP_TOOLS: usize = 9;
/// Höhe der Leiste mit allen Werkzeugen: 9 × 36, 8 Abstände, oben und unten der Rand.
pub const STRIP_H: f32 = STRIP_TOOLS as f32 * STRIP_BUTTON
    + (STRIP_TOOLS as f32 - 1.0) * STRIP_SPACING
    + 2.0 * STRIP_PAD;

/// Was die Oberfläche gerade zeigt; bestimmt, welche Felder es gibt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShellMode {
    /// 3D-Modell: Baum links, Eigenschaften rechts, Befehlsfeld unten.
    Model,
    /// Zeichnung: Baum links wie im Modell, Werkzeugfeld unter dem oberen Feld, Ansichtsfeld
    /// rechts, Hinweisfeld unten.
    Drawing,
    /// Skizze: Werkzeugleiste links, Skizzenfeld rechts, Fußfeld unten.
    Sketch,
}

/// Reiter im rechten Feld.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RightTab {
    Properties,
    Versions,
}

/// Sichtbarkeit und Reiter der Felder.
#[derive(Clone, Debug)]
pub struct ShellState {
    pub versions_visible: bool,
    pub right_tab: RightTab,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            versions_visible: true,
            right_tab: RightTab::Properties,
        }
    }
}

impl ShellState {
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
    /// Werkzeugleiste links (nur im Skizzenmodus; dort ist `left` leer).
    pub strip: Option<Rect>,
    /// Werkzeugfeld der Zeichnung (nur im Zeichnungsmodus).
    pub tool: Option<Rect>,
    /// Verlaufsleiste (nur im Modell): zwischen den Spalten über dem Befehlsfeld.
    pub history: Option<Rect>,
}

/// Breite der Seitenspalten bei Fensterbreite `content_width`. Zu kleines Fenster: die Spalten
/// werden schmaler, damit die freie Fläche [`MIN_FREE`] breit bleibt.
fn side_width(content_width: f32) -> f32 {
    let normal_free_w = content_width - 2.0 * (SHELL_INSET + theme::PANEL_WIDTH + GAP);
    if normal_free_w < MIN_FREE {
        MIN_SIDE_W.max((content_width - MIN_FREE - 2.0 * (SHELL_INSET + GAP)) / 2.0)
    } else {
        theme::PANEL_WIDTH
    }
}

/// Platz für das obere Feld zwischen den Seitenspalten bei Fensterbreite `content_width`.
pub fn between_columns(content_width: f32) -> f32 {
    content_width - 2.0 * (side_width(content_width) + SHELL_INSET + GAP)
}

/// Zeigt das obere Feld das kleine Programmzeichen? Nur wenn das ganze Feld samt Zeichen
/// (`lead` = Zeichen und Abstand) ohne Rollen zwischen die Seitenspalten passt; `base_wanted` ist
/// die Wunschbreite des Felds ohne Zeichen. Sonst bleibt das Feld wie ohne Zeichen.
pub fn top_icon_fits(content_width: f32, base_wanted: f32, lead: f32) -> bool {
    base_wanted + lead <= between_columns(content_width)
}

/// Wie [`layout_with_top_min`] für den Modus `mode`.
///
/// Modell: dazu die Verlaufsleiste ([`HISTORY_H`] hoch, so breit wie die freie Fläche) direkt
/// über dem Befehlsfeld; die freie Fläche endet darüber.
///
/// Zeichnung: linke und rechte Spalte, oberes Feld, Status und Hinweis wie im Modell; das
/// Werkzeugfeld ([`TOOL_H`] hoch, so breit wie zwischen den Spalten Platz ist) steht mittig direkt
/// unter dem oberen Feld (bzw. unter dem Hinweis einer alten Version), die freie Fläche beginnt
/// erst darunter. Das untere Feld ist dort das Hinweisfeld (gleiche Regeln wie das Befehlsfeld).
///
/// Im Skizzenmodus entfällt die linke Spalte: an ihrer Stelle steht die
/// Werkzeugleiste ([`STRIP_W`] breit, senkrecht mittig zur freien Fläche, höchstens so hoch wie
/// sie), die freie Fläche beginnt links schon nach Leiste und Abstand. Rechte Spalte (Skizzenfeld),
/// unteres Feld (Fußfeld), Status und Hinweis liegen wie im Modell. `left` ist dann ein leeres
/// Rechteck an der Ecke der Leiste.
pub fn layout_in_mode(
    content: Rect,
    mode: ShellMode,
    state: &ShellState,
    top: (f32, f32),
    bottom_width: f32,
    status_width: f32,
    read_only: bool,
) -> ShellRects {
    let mut r = layout_with_top_min(content, state, top, bottom_width, status_width, read_only);
    match mode {
        ShellMode::Model => {
            // So breit wie die freie Fläche, GAP über dem Befehlsfeld; die freie Fläche endet
            // GAP darüber. Status und Spalten bleiben, wo sie sind: Der Status liegt unter der
            // Leiste (er wächst höchstens bis zur Oberkante des Befehlsfelds), die Spalten
            // daneben.
            let bottom = r.bottom.top() - GAP;
            let history = Rect::from_min_max(
                pos2(r.free.left(), bottom - HISTORY_H),
                pos2(r.free.right(), bottom),
            );
            r.free.max.y = r.free.max.y.min(history.top() - GAP).max(r.free.min.y);
            r.history = Some(history);
            return r;
        }
        ShellMode::Drawing => {
            let above = r.banner.map_or(r.top.bottom(), |b| b.bottom());
            // So breit wie zwischen den Spalten Platz ist (wie der Hinweis), mindestens 120.
            let avail = content.width() - 2.0 * (r.left.width() + SHELL_INSET + GAP);
            let w = avail
                .max(MIN_FIELD_W)
                .min(content.width() - 2.0 * SHELL_INSET);
            let tool = Rect::from_min_size(
                pos2(content.center().x - w / 2.0, above + GAP),
                vec2(w, TOOL_H),
            );
            r.free.min.y = r.free.min.y.max(tool.bottom() + GAP);
            r.tool = Some(tool);
            return r;
        }
        ShellMode::Sketch => {}
    }
    let left = content.left() + SHELL_INSET;
    r.free.min.x = left + STRIP_W + GAP;
    // Das Fußfeld darf breiter sein als zwischen den Spalten: links steht nur die schmale Leiste,
    // und die rechte Spalte endet über ihm. So passen Zustand und „Skizze beenden“ auch in
    // schmalen Fenstern; der Status weicht wie im Modell bis GAP vor dem Fuß zurück.
    let avail = content.width() - 2.0 * (SHELL_INSET + STRIP_W + GAP);
    let foot_w = bottom_width
        .min(avail.max(MIN_FIELD_W))
        .min(content.width() - 2.0 * SHELL_INSET);
    r.bottom = Rect::from_center_size(r.bottom.center(), vec2(foot_w, BOTTOM_H));
    let status_right = (r.bottom.left() - GAP).max(r.status.left());
    r.status.max.x = r.status.max.x.min(status_right);
    let h = STRIP_H.min(r.free.height());
    let strip = Rect::from_min_size(pos2(left, r.free.center().y - h / 2.0), vec2(STRIP_W, h));
    r.left = Rect::from_min_size(strip.min, vec2(0.0, 0.0));
    r.strip = Some(strip);
    r
}

/// Werkzeugfeld der Zeichnung nach seinem Inhalt: Breite `wanted`, mindestens [`MIN_FIELD_W`],
/// höchstens die des Rechtecks `tool`; gleiche Mitte, gleiche Höhe.
pub fn fit_tool(tool: Rect, wanted: f32) -> Rect {
    let w = wanted.clamp(MIN_FIELD_W, tool.width().max(MIN_FIELD_W));
    Rect::from_center_size(tool.center(), vec2(w, tool.height()))
}

/// Berechnet die Geometrie deterministisch aus Konstanten (kein Messen, keine Rückkopplung).
pub fn layout(
    content: Rect,
    state: &ShellState,
    top_width: f32,
    bottom_width: f32,
    status_width: f32,
    read_only: bool,
) -> ShellRects {
    layout_with_top_min(
        content,
        state,
        (top_width, 0.0),
        bottom_width,
        status_width,
        read_only,
    )
}

/// Wie [`layout`]; `(top_width, top_min)`: Wunschbreite des oberen Felds und die Breite, die es
/// mindestens braucht (Menü und ein Tab). Passt `top_min` nicht zwischen die Seitenspalten
/// (schmale Fenster), wird das obere Feld so breit wie nötig und die Seitenspalten beginnen erst
/// darunter. So überdeckt das obere Feld nie eine Spalte (Hand-Prüfung Haltepunkt 2).
pub fn layout_with_top_min(
    content: Rect,
    _state: &ShellState,
    (top_width, top_min): (f32, f32),
    bottom_width: f32,
    status_width: f32,
    read_only: bool,
) -> ShellRects {
    // Die Seitenspalten sind immer vorhanden (Baum links, Eigenschaften rechts); der
    // Umschalter für Versionen ändert nur ihren Inhalt, nicht die Geometrie.
    let banner_extra = if read_only { BANNER_H + GAP } else { 0.0 };
    let normal_free_h =
        content.height() - (2.0 * SHELL_INSET + TOP_H + BOTTOM_H + 2.0 * GAP + banner_extra);
    // Zu kleines Fenster: Seitenspalten schmaler, damit die freie Fläche 200 breit bleibt.
    let normal_free_w = content.width() - 2.0 * (SHELL_INSET + theme::PANEL_WIDTH + GAP);
    let side_w = side_width(content.width());

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
    let avail = between_columns(content.width());
    let max_w = content.width() - 2.0 * SHELL_INSET;
    let mid_w = |wanted: f32| wanted.min(avail.max(MIN_FIELD_W)).min(max_w);

    // Braucht das obere Feld mehr Platz als zwischen den Spalten ist, steht es über ihnen.
    let top_over_columns = top_min > avail.max(MIN_FIELD_W);
    let top_w = if top_over_columns {
        top_width.max(top_min).min(max_w)
    } else {
        mid_w(top_width)
    };
    let top = Rect::from_center_size(
        pos2(
            content.center().x,
            content.top() + SHELL_INSET + TOP_H / 2.0,
        ),
        vec2(top_w, TOP_H),
    );
    let bottom = Rect::from_center_size(
        pos2(
            content.center().x,
            content.bottom() - SHELL_INSET - BOTTOM_H / 2.0,
        ),
        vec2(mid_w(bottom_width), BOTTOM_H),
    );
    // Status: so breit wie sein Text, höchstens bis GAP vor dem unteren Feld. Er liegt unter der
    // freien Fläche und unter der linken Spalte, breiter verdeckt er also nichts.
    let status_left = content.left() + SHELL_INSET;
    let status = Rect::from_min_size(
        pos2(status_left, content.bottom() - SHELL_INSET - STATUS_H),
        vec2(
            status_width.min(bottom.left() - GAP - status_left).max(0.0),
            STATUS_H,
        ),
    );

    let col_top = if top_over_columns {
        top.bottom() + GAP
    } else {
        content.top() + SHELL_INSET
    };
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

    // Hinweis: die ganze Breite zwischen den Spalten (mittig wie das obere Feld), damit der Text
    // möglichst in eine Zeile passt; er berührt die Spalten nie.
    let banner = read_only.then(|| {
        let w = mid_w(f32::INFINITY);
        Rect::from_min_size(
            pos2(content.center().x - w / 2.0, top.bottom() + GAP),
            vec2(w, BANNER_H),
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
        strip: None,
        tool: None,
        history: None,
    }
}

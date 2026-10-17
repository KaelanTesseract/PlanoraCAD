use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::{fonts, sketch_view, theme};
use egui::epaint::Vertex;
use egui::{
    Align2, Color32, FontId, Mesh, PointerButton, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2,
};
use glam::{Vec2, Vec3};
use planora_render::gpu::ViewportCallback;
use planora_render::{
    CubePiece, CubeRegion, INSET, OrbitCamera, Palette, Pick, ViewFace, cube_pieces_visible,
    current_region, face_text_frame, pieces_hit_test,
};

/// Kantenlänge des Würfelfelds (Variante C in Größe B).
pub const CUBE_SIZE: f32 = 84.0;
/// Abstand des Würfelfelds zum Rand der freien Fläche (rechts und oben).
const CUBE_MARGIN: f32 = 12.0;
/// Durchmesser des runden Home-Knopfs.
const HOME_SIZE: f32 = 26.0;
/// So weit ragt der Home-Knopf in das Würfelfeld hinein.
const HOME_OVERLAP: f32 = 2.0;
/// Höhe des Streifens mit dem Namen der Ansicht unter dem Würfel.
const NAME_H: f32 = 16.0;
/// Bildpunkte je Würfeleinheit (Würfel ±1). Die äußerste Ecke liegt höchstens √3 Einheiten von
/// der Mitte entfernt, also 41 px: der Würfel bleibt im 84-px-Feld.
pub const CUBE_SCALE: f32 = CUBE_SIZE * 0.28;
/// Beschriftet wird nur eine Fläche, die mindestens so stark zur Kamera zeigt.
pub const LABEL_MIN_FACING: f32 = 0.3;
/// Ab hier ist die Beschriftung voll deckend; darunter blendet sie bis `LABEL_MIN_FACING` aus.
pub const LABEL_FULL_FACING: f32 = 0.5;
/// Bereiche des Würfels, die höchstens so stark zur Kamera zeigen, werden weder gemalt noch
/// angeklickt: Fast flach gesehen wären sie nur ein 1–2 px schmaler Streifen, dessen Umriss beim
/// Drehen als Linie aufblitzt. Bei 0,15 ist der schmalste Bereich (Eckendreieck) noch etwa
/// 1,7 px breit.
pub const PIECE_MIN_FACING: f32 = 0.15;
/// Ab hier ist ein Bereich voll deckend; darunter blenden Füllung und Umriss bis
/// `PIECE_MIN_FACING` aus, damit beim Drehen nichts schlagartig erscheint oder verschwindet.
pub const PIECE_FULL_FACING: f32 = 0.3;
/// Das längste Wort nimmt höchstens diesen Anteil der Flächenbreite ein.
const LABEL_FILL: f32 = 0.85;
/// Abweichung in Grad, bis zu der eine Blickrichtung als Ansicht eines Bereichs gilt.
const REGION_TOLERANCE_DEG: f32 = 2.0;

/// Farben der 3D-Szene im Graphit-Theme: Bodenraster (`viewport_grid`), Achsen und Auswahl aus
/// der Palette, damit sie zum Hintergrund `bg_viewport` passen. Auswahl der Flächen in
/// `selection`, gewählte Kanten wie die Skizzen-Auswahl (hell ein dunkleres Bernstein, das auf
/// dem hellen Grund als Linie lesbar bleibt). Flächen, Kanten und Hover behält
/// `planora_render`.
pub fn render_palette(dark: bool) -> Palette {
    let p = theme::palette(dark);
    let rgba = |c: Color32| {
        let f = |v: u8| f32::from(v) / 255.0;
        [f(c.r()), f(c.g()), f(c.b()), 1.0]
    };
    let base = if dark {
        Palette::dark()
    } else {
        Palette::light()
    };
    Palette {
        face_selected: rgba(p.selection),
        edge_selected: rgba(theme::sketch_colors(dark).selected),
        grid: rgba(p.viewport_grid),
        axis_x: rgba(p.axis_x),
        axis_y: rgba(p.axis_y),
        axis_z: rgba(p.axis_z),
        ..base
    }
}

/// Navigationswürfel rechts oben im freien Rechteck `free` (Rand 12 px, 84 × 84).
pub fn cube_rect(free: Rect) -> Rect {
    Rect::from_min_size(
        pos2(
            free.right() - CUBE_SIZE - CUBE_MARGIN,
            free.top() + CUBE_MARGIN,
        ),
        vec2(CUBE_SIZE, CUBE_SIZE),
    )
}

/// Runder Home-Knopf (26 × 26) links oben am Würfel, oben bündig, 2 px in das Würfelfeld hinein.
pub fn home_rect(cube: Rect) -> Rect {
    Rect::from_min_size(
        pos2(cube.left() - HOME_SIZE + HOME_OVERLAP, cube.top()),
        vec2(HOME_SIZE, HOME_SIZE),
    )
}

/// Streifen unter dem Würfel (84 × 16) für den Namen der aktuellen Ansicht.
pub fn name_rect(cube: Rect) -> Rect {
    Rect::from_min_size(cube.left_bottom(), vec2(cube.width(), NAME_H))
}

/// Würfeleinheiten (x rechts, y oben, Mitte 0) → Bildschirm im Würfelfeld `cube`.
pub fn cube_to_screen(cube: Rect, p: Vec2) -> Pos2 {
    let c = cube.center();
    pos2(c.x + p.x * CUBE_SCALE, c.y - p.y * CUBE_SCALE)
}

/// Bildschirm → Würfeleinheiten (Umkehrung von [`cube_to_screen`]).
fn screen_to_cube(cube: Rect, p: Pos2) -> Vec2 {
    let c = cube.center();
    Vec2::new((p.x - c.x) / CUBE_SCALE, -(p.y - c.y) / CUBE_SCALE)
}

/// Affine Abbildung der Flächenebene auf den Bildschirm: Punkt `(u, v)` der Fläche (Würfel-
/// einheiten, `u` entlang der Leserichtung „rechts“, `v` entlang „oben“, Mitte der Fläche 0)
/// liegt bei `origin + right·u + up·v`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceFrame {
    /// Mitte der Fläche auf dem Bildschirm.
    pub origin: Pos2,
    /// Bild von „rechts“ (eine Würfeleinheit), Bildschirm-y nach unten.
    pub right: egui::Vec2,
    /// Bild von „oben“ (eine Würfeleinheit), Bildschirm-y nach unten.
    pub up: egui::Vec2,
    /// Wie stark die Fläche zur Kamera zeigt (1 = frontal, ≤ 0 = abgewandt).
    pub facing: f32,
}

impl FaceFrame {
    /// Bildschirmpunkt zum Flächenpunkt `(u, v)`.
    pub fn apply(&self, u: f32, v: f32) -> Pos2 {
        self.origin + self.right * u + self.up * v
    }
}

/// Abbildung der Fläche `face` auf den Bildschirm: Leserichtungen aus `face_text_frame`, dieselbe
/// orthografische Projektion wie `cube_pieces`. Die Schrift liegt damit auf der Fläche, wird mit
/// ihr gedreht und verkürzt und ist nie gespiegelt (`rechts × oben` = Normale).
pub fn face_label_transform(camera: &OrbitCamera, face: ViewFace, cube: Rect) -> FaceFrame {
    let view = camera.view();
    let to_view = |v: Vec3| view.transform_vector3(v);
    let dir = |v: Vec3| {
        let w = to_view(v);
        vec2(w.x * CUBE_SCALE, -w.y * CUBE_SCALE)
    };
    let (right, up) = face_text_frame(face);
    let center = to_view(CubeRegion::Face(face).normal());
    FaceFrame {
        origin: cube_to_screen(cube, Vec2::new(center.x, center.y)),
        right: dir(right),
        up: dir(up),
        facing: center.z,
    }
}

/// Gemalte und anklickbare Bereiche: deutlich zugewandt (`PIECE_MIN_FACING`) und mit endlichen
/// Zahlen (eine ungültige Kamera liefert NaN-Stücke). Malen und Klicken nutzen dieselbe Liste,
/// ein nicht gemalter Bereich lässt sich also auch nicht treffen.
pub fn visible_pieces(camera: &OrbitCamera) -> Vec<CubePiece> {
    cube_pieces_visible(camera, PIECE_MIN_FACING)
        .into_iter()
        .filter(|p| p.depth.is_finite() && p.corners.iter().all(|c| c.is_finite()))
        .collect()
}

/// Anklickbare Bereiche: die gemalten ([`visible_pieces`]), die mindestens halb deckend zu sehen
/// sind (`piece_opacity` ≥ 0,5). Ein gerade erst einblendender, kaum sichtbarer Bereich am Rand
/// fängt also keinen Klick und kein Hover.
pub fn clickable_pieces(camera: &OrbitCamera) -> Vec<CubePiece> {
    visible_pieces(camera)
        .into_iter()
        .filter(|p| piece_opacity(p.depth) >= 0.5)
        .collect()
}

/// Bereich des Würfels unter dem Bildschirmpunkt `p` (Würfelfeld `cube`), aus derselben Liste
/// wie Hover und Klick in der Ansicht ([`clickable_pieces`]).
pub fn cube_region_at(camera: &OrbitCamera, cube: Rect, p: Pos2) -> Option<CubeRegion> {
    pieces_hit_test(&clickable_pieces(camera), screen_to_cube(cube, p))
}

/// Weicher Übergang 0 → 1 für `x` von `lo` bis `hi` (Smoothstep, außerhalb geklemmt).
fn fade(x: f32, lo: f32, hi: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Deckkraft eines Bereichs, der `facing` zur Kamera zeigt (0 an der Schwelle, 1 ab
/// `PIECE_FULL_FACING`).
pub fn piece_opacity(facing: f32) -> f32 {
    fade(facing, PIECE_MIN_FACING, PIECE_FULL_FACING)
}

/// Deckkraft der Beschriftung einer Fläche, die `facing` zur Kamera zeigt.
pub fn label_opacity(facing: f32) -> f32 {
    fade(facing, LABEL_MIN_FACING, LABEL_FULL_FACING)
}

/// Schrift der Flächenbeschriftung und ihr Maßstab (Würfeleinheiten je Schriftpunkt): Das
/// längste der sechs Wörter nimmt `LABEL_FILL` der Flächenbreite ein; die Schriftgröße ist so
/// gewählt, dass das in der Vorderansicht etwa einem Bildpunkt je Schriftpunkt entspricht.
fn label_font(painter: &egui::Painter) -> (FontId, f32) {
    let widest = |font: &FontId| {
        ViewFace::ALL
            .iter()
            .map(|f| {
                painter
                    .layout_no_wrap(f.label().to_string(), font.clone(), Color32::PLACEHOLDER)
                    .mesh_bounds
                    .width()
            })
            .fold(1.0_f32, f32::max)
    };
    let face_width = LABEL_FILL * 2.0 * INSET;
    let probe = FontId::new(12.0, fonts::medium());
    let size = (12.0 * face_width * CUBE_SCALE / widest(&probe) * 2.0).floor() / 2.0;
    let font = FontId::new(size.max(4.0), fonts::medium());
    let k = face_width / widest(&font);
    (font, k)
}

/// Beschriftung `text` als Netz auf der Fläche: Die Glyphen-Netze der egui-Textanordnung werden
/// Eckpunkt für Eckpunkt durch die Flächenabbildung `frame` gelegt (Textmitte = Flächenmitte).
fn face_label_mesh(painter: &egui::Painter, frame: &FaceFrame, text: &str, color: Color32) -> Mesh {
    let (font, k) = label_font(painter);
    let galley = painter.layout_no_wrap(text.to_string(), font, color);
    let center = galley.mesh_bounds.center();
    // Die Netze tragen Texel-Koordinaten; die Zeichenfläche erwartet 0…1.
    let [w, h] = painter.fonts(|f| f.font_image_size());
    let uv_scale = vec2(1.0 / w.max(1) as f32, 1.0 / h.max(1) as f32);
    let mut mesh = Mesh::default();
    for row in &galley.rows {
        let visuals = &row.visuals;
        let base = mesh.vertices.len() as u32;
        mesh.indices
            .extend(visuals.mesh.indices.iter().map(|i| i + base));
        for (i, v) in visuals.mesh.vertices.iter().enumerate() {
            let g = row.pos + v.pos.to_vec2();
            let (u, up) = ((g.x - center.x) * k, -(g.y - center.y) * k);
            mesh.vertices.push(Vertex {
                pos: frame.apply(u, up),
                uv: (v.uv.to_vec2() * uv_scale).to_pos2(),
                color: if visuals.glyph_vertex_range.contains(&i) {
                    color
                } else {
                    v.color
                },
            });
        }
    }
    mesh
}

/// Grundfarbe eines Bereichs: Flächen in ihrer Achse getönt, Kanten und Ecken neutral.
fn region_fill(colors: &theme::CubeColors, region: CubeRegion) -> Color32 {
    match region {
        CubeRegion::Face(ViewFace::Left | ViewFace::Right) => colors.face[0],
        CubeRegion::Face(ViewFace::Front | ViewFace::Back) => colors.face[1],
        CubeRegion::Face(ViewFace::Top | ViewFace::Bottom) => colors.face[2],
        CubeRegion::Edge { .. } => colors.edge,
        CubeRegion::Corner { .. } => colors.corner,
    }
}

/// Malt Würfel, Beschriftung, Namen der Ansicht und Home-Knopf.
fn paint_cube(
    painter: &egui::Painter,
    camera: &OrbitCamera,
    dark: bool,
    cube: Rect,
    hover: Option<Pos2>,
    home_hovered: bool,
) {
    let colors = theme::cube_colors(dark);
    let pieces = visible_pieces(camera);
    let hovered = hover.and_then(|p| cube_region_at(camera, cube, p));
    let current = current_region(camera, REGION_TOLERANCE_DEG);
    for piece in &pieces {
        let is_hovered = hovered == Some(piece.region);
        let is_current = current == Some(piece.region);
        let fill = match (is_current, is_hovered) {
            (true, true) => colors.highlight_hover,
            (true, false) => colors.highlight,
            (false, true) => theme::cube_hover_fill(dark, region_fill(&colors, piece.region)),
            (false, false) => region_fill(&colors, piece.region),
        };
        let points: Vec<Pos2> = piece
            .corners
            .iter()
            .map(|c| cube_to_screen(cube, *c))
            .collect();
        // Flach gesehene Bereiche blenden mit Füllung und Umriss ein und aus (kein Aufblitzen).
        let opacity = piece_opacity(piece.depth);
        painter.add(Shape::convex_polygon(
            points,
            fill.gamma_multiply(opacity),
            Stroke::new(1.0, colors.line.gamma_multiply(opacity)),
        ));
        if let CubeRegion::Face(face) = piece.region {
            let frame = face_label_transform(camera, face, cube);
            if frame.facing >= LABEL_MIN_FACING {
                let color = if is_current {
                    colors.text_on_highlight
                } else {
                    colors.text
                };
                painter.add(Shape::mesh(face_label_mesh(
                    painter,
                    &frame,
                    face.label(),
                    color.gamma_multiply(label_opacity(frame.facing)),
                )));
            }
        }
    }
    if let Some(region) = current {
        painter.text(
            name_rect(cube).center(),
            Align2::CENTER_CENTER,
            region.name(),
            FontId::proportional(10.0),
            colors.text,
        );
    }
    let home = home_rect(cube);
    painter.circle(
        home.center(),
        HOME_SIZE / 2.0 - 0.5,
        if home_hovered {
            colors.home_hover
        } else {
            colors.home_fill
        },
        Stroke::new(1.0, colors.line),
    );
    painter.text(
        home.center(),
        Align2::CENTER_CENTER,
        icons::glyph(Icon::Home),
        FontId::new(16.0, fonts::icons()),
        colors.text,
    );
}

/// Kamera auf das ganze Fenster `rect` einstellen, die Bildmitte ins freie Rechteck `free`
/// legen. Reihenfolge wichtig: erst `set_viewport`, dann `set_visible`, denn `set_viewport`
/// setzt bei Versatz 0 das sichtbare Seitenverhältnis zurück. Nach jedem fremden
/// `set_viewport` (Skizzen-Overlay) erneut aufrufen. `free` = `None`: kein Versatz.
fn fit_camera(state: &mut AppState, rect: Rect, free: Option<Rect>) {
    state.camera.set_viewport(rect.width(), rect.height());
    match free {
        Some(f) => state.camera.set_visible(
            Vec2::new(rect.width(), rect.height()),
            (
                Vec2::new(f.min.x - rect.min.x, f.min.y - rect.min.y),
                Vec2::new(f.width(), f.height()),
            ),
        ),
        None => state.camera.clear_visible(),
    }
}

fn keyboard_focus_id() -> egui::Id {
    egui::Id::new("planora-keyboard-focus-last-frame")
}

/// Merkt sich am Ende eines Frames, ob ein Eingabefeld die Tastatur hatte. Wird von der App nach
/// dem Aufbau aller Bereiche aufgerufen.
pub fn remember_keyboard_focus(ctx: &egui::Context) {
    let focused = ctx.egui_wants_keyboard_input();
    ctx.data_mut(|d| d.insert_temp(keyboard_focus_id(), focused));
}

/// Hat ein Eingabefeld die Tastatur (in diesem oder im vorigen Frame)? Dann sind Tastenkürzel
/// der Ansicht gesperrt. egui nimmt bei Escape den Fokus schon zu Beginn des Frames weg; deshalb
/// zählt auch, ob im vorigen Frame ein Eingabefeld (z. B. der Name) die Tastatur hatte.
pub fn typing(ctx: &egui::Context) -> bool {
    ctx.egui_wants_keyboard_input()
        || ctx
            .data(|d| d.get_temp::<bool>(keyboard_focus_id()))
            .unwrap_or(false)
}

/// 3D-Ansicht im ganzen verfügbaren Bereich ohne freies Rechteck (siehe [`show_in`]).
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    show_in(ui, state, None);
}

/// 3D-Ansicht: Navigation, Auswahl, Zeichnen und Navigationswürfel. Im Skizzen-Modus übernimmt
/// [`sketch_view::show_overlay`] die Eingaben (Orbit gesperrt, Verschieben und Zoomen bleiben).
///
/// Die Ansicht füllt den ganzen verfügbaren Bereich (unter den schwebenden Feldern); `free` ist
/// das freie Rechteck zwischen den Feldern in Bildschirmkoordinaten. Dort liegen die Bildmitte
/// der Kamera und der Würfel. Auswahl, Hover, Orbit, Zoom und Verschieben gelten im ganzen
/// Bereich; über einem Feld meldet egui die Ansicht nicht als „hovered“, ein dort begonnenes
/// Ziehen oder ein Klick erreicht sie also nicht. Ein in der Ansicht begonnenes Ziehen läuft
/// über Felder weiter.
pub fn show_in(ui: &mut egui::Ui, state: &mut AppState, free: Option<Rect>) {
    let sketching = state.sketch_session.is_some();
    let sense = if sketching {
        Sense::hover()
    } else {
        Sense::click_and_drag()
    };
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), sense);
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::viewport_background(state.dark));
    // Freies Rechteck auf den Bereich begrenzen; leer oder zu klein zählt als „kein Feld“.
    let free = free
        .map(|f| f.intersect(rect))
        .filter(|f| f.width() >= 1.0 && f.height() >= 1.0);
    fit_camera(state, rect, free);
    // Neuer Tab oder geladener Stand: einmal in das jetzt bekannte freie Rechteck einpassen. Im
    // Skizzen-Modus bleibt die Skizzenansicht; nachgeholt wird dann nach dem Beenden.
    if state.fit_pending && free.is_some() && !sketching {
        state.frame_all();
        state.fit_pending = false;
    }
    // Gerade geöffnete Skizze: mit dem jetzt bekannten freien Rechteck des Skizzenmodus
    // nachpassen (endet von selbst, siehe `refit_sketch_view`).
    if sketching {
        state.refit_sketch_view();
    }
    // Platz für die 3D-Szene; sie wird am Ende mit der endgültigen Kamera eingesetzt, damit die
    // Skizze darüber liegt.
    let scene_slot = painter.add(Shape::Noop);
    // Fertige Skizzen als dünne Linien über der Szene; ihr Platz wird ebenfalls erst am Ende mit
    // der endgültigen Kamera gefüllt.
    let sketches_slot = painter.add(Shape::Noop);

    let cube = cube_rect(free.unwrap_or(rect));
    let mut over_view = response.hovered();
    if sketching {
        // Das Overlay rechnet im ganzen Bereich (Auswahl und Fangen mit der vollen Größe) und
        // ruft selbst `set_viewport`; danach das freie Rechteck erneut setzen.
        over_view |= sketch_view::show_overlay(ui, rect, state);
        fit_camera(state, rect, free);
    } else {
        // Navigation
        let pan = response.dragged_by(PointerButton::Middle)
            || (response.dragged_by(PointerButton::Primary) && ui.input(|i| i.modifiers.shift));
        if pan {
            let d = response.drag_motion();
            state.camera.pan(d.x, d.y, rect.height());
        } else if response.dragged_by(PointerButton::Primary) {
            let d = response.drag_motion();
            state.camera.orbit(d.x, d.y);
        }
    }

    // Navigationswürfel und Home-Knopf: eigene Schaltflächen über der Ansicht (auch im
    // Skizzen-Modus). Sie nehmen Klicks und Ziehen auf, damit beides weder dreht noch wählt.
    let cube_response = ui.interact(
        cube,
        ui.id().with("planora-view-cube"),
        Sense::click_and_drag(),
    );
    let cube_hover = cube_response.hover_pos();
    let camera = state.camera;
    let region_at = |p: Pos2| cube_region_at(&camera, cube, p);
    let hovered_region = cube_hover.and_then(region_at);
    if cube_response.clicked()
        && let Some(region) = cube_response
            .interact_pointer_pos()
            .and_then(region_at)
            .or(hovered_region)
    {
        state.camera.set_view_region(region);
    }
    if let Some(region) = hovered_region {
        cube_response.on_hover_text(format!("Ansicht: {}", region.name()));
    }
    let home_response = ui.interact(
        home_rect(cube),
        ui.id().with("planora-view-home"),
        Sense::click_and_drag(),
    );
    home_response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Home"));
    if home_response.clicked() {
        state.camera.home();
        state.frame_all();
    }
    let home_hovered = home_response.hovered();
    // Mausrad-Zoom (Modell und Skizze) auch über Würfel und Home-Knopf.
    if over_view || cube_hover.is_some() || home_hovered {
        let scroll = ui.input(|i| i.smooth_scroll_delta().y);
        if scroll != 0.0 {
            state.camera.zoom(scroll);
        }
    }
    home_response.on_hover_text("Home: Grundansicht (Isometrisch)");

    if !sketching {
        // Auswahl (nicht über dem Würfel; dort ist die Ansicht nicht „hovered“)
        match response.hover_pos() {
            Some(p) => {
                let cursor = Vec2::new(p.x - rect.left(), p.y - rect.top());
                let viewport = Vec2::new(rect.width(), rect.height());
                let hit = state.pick(cursor, viewport);
                state.set_hover(hit);
                if response.clicked() {
                    match hit {
                        Some(Pick::Edge(e)) if state.edge_picking => {
                            let extend = ui.input(|i| i.modifiers.shift);
                            state.toggle_edge_mark(e, extend);
                        }
                        _ => state.set_selection(hit),
                    }
                }
            }
            None => state.set_hover(None),
        }
        if !typing(ui.ctx()) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.set_selection(None);
            state.clear_edge_marks();
        }
    }

    // 3D-Szene
    let cam = state.camera;
    painter.set(
        scene_slot,
        eframe::egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                view_proj: cam.view_proj().to_cols_array_2d(),
                light_dir: cam.light_dir().to_array(),
                scene: state.scene.clone(),
                scene_version: state.scene_version,
            },
        ),
    );
    painter.set(
        sketches_slot,
        Shape::Vec(sketch_view::finished_sketch_shapes(state, rect)),
    );

    // Würfel mit der endgültigen Kamera zeichnen (nach einem Klick schon in der neuen Ansicht).
    paint_cube(
        &painter,
        &state.camera,
        state.dark,
        cube,
        cube_hover,
        home_hovered,
    );
}

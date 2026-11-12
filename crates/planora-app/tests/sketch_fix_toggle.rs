//! Skizzen 9, Task 7: „Fest“ ist ein Schalter wie Fixieren/Lösen in Fusion. Ein eingefügtes
//! (festes) SVG lässt sich mit Rahmenauswahl und „Fest“ in einem Schritt lösen.

use planora_app::sketch_state::ConstraintAction as A;
use planora_app::state::{AppState, StandardPlane};
use planora_exchange::import2d::{Element2d, Import2d, Layer2d};
use planora_feature::SketchPlane;
use planora_i18n::{german, with_lang};
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::import::{InsertOptions, Placement};
use planora_sketch::model::Sketch;
use planora_sketch::text_shape::{TextShape, TextShapeError, TextShaper, TextStyle};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Kein Text im Quadrat: Der Setzer wird nie gebraucht.
struct NoText;

impl TextShaper for NoText {
    fn shape(&self, _: &str, _: &TextStyle) -> Result<TextShape, TextShapeError> {
        Err(TextShapeError::Empty)
    }
}

fn square_import() -> Import2d {
    Import2d {
        layers: vec![Layer2d {
            name: "0".into(),
            elements: vec![
                Element2d::Line(v(0.0, 0.0), v(10.0, 0.0)),
                Element2d::Line(v(10.0, 0.0), v(10.0, 10.0)),
                Element2d::Line(v(10.0, 10.0), v(0.0, 10.0)),
                Element2d::Line(v(0.0, 10.0), v(0.0, 0.0)),
            ],
        }],
        ..Default::default()
    }
}

fn fixed_count(s: &AppState) -> usize {
    s.session_sketch()
        .unwrap()
        .constraints()
        .filter(|(_, c)| matches!(c.kind, K::Fixed(..)))
        .count()
}

fn state_with_fixed_square() -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let opt = InsertOptions {
        fixed: true,
        ..Default::default()
    };
    let sk: &mut Sketch = s.session_sketch_mut().unwrap().1;
    sk.insert_geometry(&square_import(), &Placement::default(), &opt, &NoText)
        .unwrap();
    s.observe_sketch(false);
    s
}

fn box_select_all(s: &mut AppState) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_box(v(-5.0, -5.0), false);
    se.box_to(v(15.0, 15.0));
    se.end_box(sk, &|p| Some(p));
}

fn press_fixed(s: &mut AppState) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.constraint_button(sk, A::Fixed).unwrap();
    s.observe_sketch(false);
}

#[test]
fn fixed_tool_on_fixed_box_selection_releases_in_one_undo_step() {
    with_lang(&german(), || {
        let mut s = state_with_fixed_square();
        assert_eq!(fixed_count(&s), 4);
        let undo = s.sketch_history().unwrap().undo_len();
        box_select_all(&mut s);
        press_fixed(&mut s);
        assert_eq!(fixed_count(&s), 0, "alle gelöst");
        assert_eq!(s.sketch_history().unwrap().undo_len(), undo + 1);
        assert_eq!(s.sketch_undo_label().as_deref(), Some("Fixierung aufheben"));
        assert!(s.undo_sketch());
        assert_eq!(fixed_count(&s), 4, "ein Rückgängig stellt alle wieder her");
        assert_eq!(s.sketch_history().unwrap().undo_len(), undo);
    });
}

#[test]
fn fixed_tool_on_mixed_selection_fixes_the_rest() {
    with_lang(&german(), || {
        let mut s = state_with_fixed_square();
        // Eine Ecke lösen, dann alles wählen: „Fest“ fixiert nur die fehlende wieder.
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            let p = sk
                .constraints()
                .find_map(|(i, c)| matches!(c.kind, K::Fixed(..)).then_some(i))
                .unwrap();
            sk.remove_constraint(p).unwrap();
            se.selection.clear();
        }
        s.observe_sketch(false);
        assert_eq!(fixed_count(&s), 3);
        box_select_all(&mut s);
        press_fixed(&mut s);
        assert_eq!(fixed_count(&s), 4);
    });
}

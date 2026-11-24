use super::*;

fn win() -> DetachedWindow {
    DetachedWindow {
        x: -1500.0,
        y: 40.0,
        width: 400.0,
        height: 600.0,
    }
}

#[test]
fn place_keys() {
    assert_eq!(Place::Tree.key(), "tree");
    assert_eq!(Place::Right.key(), "right");
    assert_eq!(Place::SketchSide.key(), "sketch_side");
    assert_eq!(Place::Dialog("blend").key(), "dialog:blend");
}

#[test]
fn keys_per_kind_are_listed() {
    for c in crate::plane_dialog::PlaneCommand::ALL {
        assert!(DIALOG_KEYS.contains(&c.dock_key()), "{c:?}");
    }
}

#[test]
fn with_state_lends_installed_state() {
    let ctx = egui::Context::default();
    assert!(!is_installed(&ctx));
    assert_eq!(with_state(&ctx, |_| 1), None, "ohne install nichts");
    let mut s = DetachState::default();
    s.set(Place::Tree, win());
    install(&ctx, &mut s);
    assert_eq!(s.get(Place::Tree), None, "abgelegt");
    assert!(is_installed(&ctx));
    let inner = with_state(&ctx, |st| {
        st.undock(Place::Tree);
        // Ausgeliehen: ein verschachtelter Aufruf findet keinen Zustand.
        (is_installed(&ctx), with_state(&ctx, |_| ()))
    });
    assert_eq!(inner, Some((false, None)));
    uninstall(&ctx, &mut s);
    assert_eq!(s.get(Place::Tree), None, "Änderung zurück");
    assert!(s.take_dirty());
    assert!(!is_installed(&ctx));
}

#[test]
fn should_detach_missing_monitor_keeps_docked() {
    assert_eq!(should_detach(Some(win()), |_| Some(false)), None);
    assert_eq!(should_detach(None, |_| Some(true)), None);
}

#[test]
fn should_detach_unknown_platform_trusts_saved() {
    assert_eq!(should_detach(Some(win()), |_| None), Some(win()));
    assert_eq!(should_detach(Some(win()), |_| Some(true)), Some(win()));
}

#[test]
fn field_decision_checks_monitor_only_when_place_appears() {
    let off = |_| Some(false);
    let on = |_| Some(true);
    // Kein Eintrag: angedockt.
    assert!(!field_detached(Some((None, true)), None, on));
    // Erscheint (kein Gedächtnis): der Monitor entscheidet.
    assert!(field_detached(None, Some(win()), on));
    assert!(!field_detached(None, Some(win()), off));
    // Stand abgedockt: bleibt es, auch wenn die neue Lage außerhalb liegt.
    let moved = DetachedWindow { x: -9e4, ..win() };
    assert!(field_detached(Some((Some(win()), true)), Some(moved), off));
    // Stand angedockt trotz Eintrag (Monitor fehlt): bleibt, bis sich der Eintrag ändert.
    assert!(!field_detached(Some((Some(win()), false)), Some(win()), on));
    assert!(field_detached(Some((Some(win()), false)), Some(moved), on));
    assert!(field_detached(Some((None, false)), Some(win()), on));
}

#[test]
fn moved_by_system_tolerance() {
    let w = win();
    assert!(!moved_by_system(
        w,
        DetachedWindow { x: w.x + 1.5, ..w },
        true
    ));
    assert!(moved_by_system(
        w,
        DetachedWindow { x: w.x + 3.0, ..w },
        false
    ));
    let taller = DetachedWindow {
        height: w.height + 40.0,
        ..w
    };
    assert!(!moved_by_system(w, taller, false), "Dialoge: nur die Lage");
    assert!(moved_by_system(w, taller, true));
}

#[test]
fn uninstall_keeps_dirty_mark_of_placeholder() {
    let ctx = egui::Context::default();
    let mut s = DetachState::default();
    install(&ctx, &mut s);
    assert!(s.windows().is_none(), "verliehen");
    s.mark_dirty();
    uninstall(&ctx, &mut s);
    assert!(s.windows().is_some());
    assert!(s.take_dirty(), "neu schreiben");
}

#[test]
fn physical_round_trip_125() {
    let r = egui::Rect::from_min_size(egui::pos2(-120.3, 33.7), egui::vec2(320.0, 480.5));
    let back = to_points(to_physical(r, 1.25), 1.25);
    assert!((back.min - r.min).length() < 0.01);
    assert!((back.max - r.max).length() < 0.01);
}

#[test]
fn state_undock_marks_dirty() {
    let mut s = DetachState::default();
    assert!(!s.take_dirty());
    s.set(Place::Tree, win());
    assert_eq!(s.get(Place::Tree), Some(win()));
    assert!(s.take_dirty());
    assert!(!s.take_dirty());
    s.set(Place::Tree, win());
    assert!(!s.take_dirty(), "gleicher Wert ist keine Änderung");
    s.undock(Place::Tree);
    assert_eq!(s.get(Place::Tree), None);
    assert!(s.take_dirty());
    s.undock(Place::Tree);
    assert!(!s.take_dirty());
}

#[test]
fn save_writes_only_detached_field() {
    let dir = std::env::temp_dir().join(format!("planora-detach-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    let mut s = DetachState::default();
    s.set(Place::Right, win());
    assert!(s.save(&path));
    let c = Config::load_from(&path);
    assert_eq!(DetachState::from_config(&c).get(Place::Right), Some(win()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn failed_save_keeps_change_pending() {
    let dir = std::env::temp_dir().join(format!("planora-detach-dirty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Elternpfad ist eine Datei: Schreiben muss scheitern.
    let blocker = dir.join("datei");
    std::fs::write(&blocker, b"x").unwrap();
    let bad = blocker.join("config.json");
    let mut s = DetachState::default();
    s.set(Place::Tree, win());
    s.save_if_dirty(&bad);
    assert!(s.take_dirty(), "gescheitertes Speichern bleibt vorgemerkt");
    s.mark_dirty();
    let good = dir.join("config.json");
    s.save_if_dirty(&good);
    assert!(!s.take_dirty(), "gespeichert");
    let c = Config::load_from(&good);
    assert_eq!(DetachState::from_config(&c).get(Place::Tree), Some(win()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cut_out_leaves_the_rest_of_the_strip() {
    let strip = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 24.0));
    let knob = egui::Rect::from_min_max(egui::pos2(40.0, 2.0), egui::pos2(60.0, 22.0));
    let pieces = cut_out(strip, knob);
    let area: f32 = pieces.iter().map(|r| r.area()).sum();
    assert!(
        (area - (strip.area() - knob.area())).abs() < 1e-3,
        "{pieces:?}"
    );
    assert!(pieces.iter().all(|r| !r.intersects(knob.shrink(0.5))));
    // Ohne Überschneidung bleibt der Streifen ganz.
    let off = knob.translate(egui::vec2(200.0, 0.0));
    assert_eq!(cut_out(strip, off), vec![strip]);
}

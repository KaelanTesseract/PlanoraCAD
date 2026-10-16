//! Geometrie und Zustand der schwebenden Felder (reine Funktionen, ohne egui-Bild).

use egui::{Rect, pos2, vec2};
use planora_app::shell::{
    BANNER_H, BOTTOM_H, GAP, RightTab, SHELL_INSET, STRIP_H, STRIP_W, ShellMode, ShellRects,
    ShellState, TOOL_H, TOP_H, layout, layout_in_mode, layout_with_top_min,
};
use planora_app::viewport::cube_rect;

fn window(w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h))
}

fn rects(w: f32, h: f32, state: &ShellState, read_only: bool) -> ShellRects {
    layout(window(w, h), state, 600.0, 520.0, 200.0, read_only)
}

/// Echte Überlappung (bloßes Berühren der Kanten zählt nicht).
fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

fn named(r: &ShellRects) -> Vec<(&'static str, Rect)> {
    let mut v = vec![
        ("top", r.top),
        ("left", r.left),
        ("right", r.right),
        ("bottom", r.bottom),
        ("status", r.status),
    ];
    if let Some(b) = r.banner {
        v.push(("banner", b));
    }
    v
}

#[test]
fn no_fields_overlap_at_common_sizes() {
    for (w, h) in [
        (640.0, 480.0),
        (700.0, 500.0),
        (800.0, 600.0),
        (1100.0, 700.0),
        (1400.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
    ] {
        let content = window(w, h);
        for read_only in [false, true] {
            let r = rects(w, h, &ShellState::default(), read_only);
            let all = named(&r);
            for (name, rect) in &all {
                assert!(
                    content.contains_rect(*rect),
                    "{name} liegt bei {w}x{h} außerhalb: {rect:?}"
                );
            }
            for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    assert!(
                        !overlaps(all[i].1, all[j].1),
                        "{} und {} überlappen bei {w}x{h}: {:?} / {:?}",
                        all[i].0,
                        all[j].0,
                        all[i].1,
                        all[j].1
                    );
                }
            }
        }
    }
}

#[test]
fn free_rect_matches_the_documented_margins_at_1400x900() {
    let r = rects(1400.0, 900.0, &ShellState::default(), false);
    assert_eq!(r.free.left(), 278.0);
    assert_eq!(r.free.top(), 76.0);
    assert_eq!(r.free.bottom(), 900.0 - 84.0);
    assert_eq!(r.free.right(), 1400.0 - 278.0);
    assert_eq!(r.left.width(), 250.0);
    assert_eq!(r.right.width(), 250.0);
    assert_eq!(r.left.top(), SHELL_INSET);
    assert_eq!(r.left.bottom(), r.free.bottom());
}

#[test]
fn free_rect_never_smaller_than_200_square() {
    for (w, h) in [(640.0, 480.0), (400.0, 400.0), (300.0, 300.0)] {
        let content = window(w, h);
        let r = rects(w, h, &ShellState::default(), true);
        assert!(r.free.width() >= 200.0, "{w}x{h}: {:?}", r.free);
        assert!(r.free.height() >= 200.0, "{w}x{h}: {:?}", r.free);
        assert!(content.contains_rect(r.free), "{w}x{h}: {:?}", r.free);
        for (name, rect) in named(&r) {
            assert!(
                content.contains_rect(rect),
                "{name} liegt bei {w}x{h} außerhalb: {rect:?}"
            );
        }
    }
}

#[test]
fn read_only_banner_sits_below_the_top_field() {
    let state = ShellState::default();
    assert!(rects(1400.0, 900.0, &state, false).banner.is_none());
    let r = rects(1400.0, 900.0, &state, true);
    let b = r.banner.expect("Hinweisfeld bei schreibgeschützt");
    assert!(b.top() >= r.top.bottom() + GAP - 0.01);
    assert!(!overlaps(b, r.top));
    assert!(!overlaps(b, r.left));
    assert!(!overlaps(b, r.right));
    assert_eq!(b.center().x, r.top.center().x);
}

#[test]
fn hidden_versions_keep_the_right_column() {
    let mut state = ShellState::default();
    state.set_versions_visible(false);
    let r = rects(1400.0, 900.0, &state, false);
    assert_eq!(r.right.width(), 250.0);
    assert_eq!(r.free.right(), 1400.0 - 278.0);
}

#[test]
fn show_projects_makes_projects_visible() {
    let mut state = ShellState {
        projects_visible: false,
        ..ShellState::default()
    };
    state.show_projects();
    assert!(state.projects_visible);
}

#[test]
fn right_tab_falls_back_when_versions_hidden() {
    let mut state = ShellState {
        right_tab: RightTab::Versions,
        ..ShellState::default()
    };
    assert_eq!(state.effective_right_tab(), RightTab::Versions);
    state.set_versions_visible(false);
    assert_eq!(state.effective_right_tab(), RightTab::Properties);
}

#[test]
fn small_windows_have_documented_geometry() {
    // 400x400: Seitenspalten 160, freie Fläche 200 breit mittig, Höhe 400-76-84 = 240.
    let r = rects(400.0, 400.0, &ShellState::default(), false);
    assert_eq!(r.left.width(), 160.0);
    assert_eq!(r.right.width(), 160.0);
    assert_eq!(
        r.free,
        Rect::from_min_max(pos2(100.0, 76.0), pos2(300.0, 316.0))
    );
    // 640x480: Spalten (640-200-2*(16+12))/2 = 192, freie Fläche 220..420 (Abstand 12 zu den Spalten).
    let r = rects(640.0, 480.0, &ShellState::default(), false);
    assert_eq!(r.left.width(), 192.0);
    assert_eq!(r.free.left(), 220.0);
    assert_eq!(r.free.right(), 420.0);
    assert_eq!(r.free.top(), 76.0);
    assert_eq!(r.free.bottom(), 480.0 - 84.0);
}

#[test]
fn free_rect_clears_all_fields_at_common_sizes() {
    for (w, h) in [
        (800.0, 600.0),
        (1100.0, 700.0),
        (1400.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
    ] {
        for read_only in [false, true] {
            let r = rects(w, h, &ShellState::default(), read_only);
            for (name, rect) in named(&r) {
                assert!(
                    !overlaps(r.free, rect),
                    "free überlappt {name} bei {w}x{h} (read_only={read_only}): {:?} / {rect:?}",
                    r.free
                );
            }
        }
    }
}

#[test]
fn read_only_raises_the_free_top_by_banner_and_gap_at_1400x900() {
    // Platz für zwei Zeilen Hinweis: 16 + 48 + 12 + 40 + 12.
    let r = rects(1400.0, 900.0, &ShellState::default(), true);
    assert_eq!(r.free.top(), 128.0);
    assert_eq!(r.free.left(), 278.0);
    assert_eq!(r.free.bottom(), 900.0 - 84.0);
}

#[test]
fn banner_spans_between_the_columns_and_status_grows_to_the_bottom_field() {
    let r = rects(1400.0, 900.0, &ShellState::default(), true);
    let b = r.banner.unwrap();
    assert_eq!((b.left(), b.right()), (278.0, 1400.0 - 278.0));
    assert_eq!(b.height(), 40.0);
    // Unteres Feld 520 breit mittig (440..960): Status höchstens bis 440 − 12.
    let wide = layout(
        window(1400.0, 900.0),
        &ShellState::default(),
        600.0,
        520.0,
        2000.0,
        false,
    );
    assert_eq!(wide.bottom.left(), 440.0);
    assert_eq!(wide.status.left(), 16.0);
    assert_eq!(wide.status.right(), 428.0);
    // Ein kurzer Text bleibt schmal.
    let short = layout(
        window(1400.0, 900.0),
        &ShellState::default(),
        600.0,
        520.0,
        90.0,
        false,
    );
    assert_eq!(short.status.width(), 90.0);
    // Auch zwei Zeilen Status (bis 56 hoch) enden unter der freien Fläche.
    assert!(wide.status.bottom() - 56.0 >= wide.free.bottom());
}

/// Wie `rects`, aber mit der Mindestbreite des oberen Felds (Menü und ein Tab, etwa 431 px).
fn rects_with_top_min(w: f32, h: f32, read_only: bool) -> ShellRects {
    layout_with_top_min(
        window(w, h),
        &ShellState::default(),
        (600.0, 431.0),
        520.0,
        200.0,
        read_only,
    )
}

#[test]
fn top_field_over_the_columns_when_its_minimum_does_not_fit() {
    let h = 600.0;
    let mut w = 800.0;
    while w <= 1000.0 {
        let content = window(w, h);
        for read_only in [false, true] {
            let r = rects_with_top_min(w, h, read_only);
            let all = named(&r);
            for (name, rect) in &all {
                assert!(
                    content.contains_rect(*rect),
                    "{name} liegt bei {w}x{h} außerhalb: {rect:?}"
                );
            }
            assert!(
                content.contains_rect(r.free),
                "freie Fläche bei {w} außerhalb"
            );
            for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    assert!(
                        !overlaps(all[i].1, all[j].1),
                        "{} und {} überlappen bei {w}x{h} (schreibgeschützt {read_only}): {:?} / {:?}",
                        all[i].0,
                        all[j].0,
                        all[i].1,
                        all[j].1
                    );
                }
            }
            assert!(!overlaps(r.top, r.left), "oben/links bei {w}");
            assert!(!overlaps(r.top, r.right), "oben/rechts bei {w}");
            assert!(
                r.top.width() >= 431.0 - 0.01,
                "oberes Feld zu schmal bei {w}"
            );
            if let Some(b) = r.banner {
                assert!(b.left() >= r.left.right(), "Hinweis berührt links bei {w}");
                assert!(
                    b.right() <= r.right.left(),
                    "Hinweis berührt rechts bei {w}"
                );
                assert!(
                    r.free.top() >= b.bottom() + GAP - 0.01,
                    "freie Fläche beginnt bei {w} zu nah am Hinweis: {} / {}",
                    r.free.top(),
                    b.bottom()
                );
            }
            // Der Würfel liegt in der freien Fläche und damit unter keinem Feld.
            let cube = cube_rect(r.free);
            for (name, rect) in &all {
                assert!(
                    !overlaps(cube, *rect),
                    "Würfel liegt bei {w}x{h} unter {name}: {cube:?} / {rect:?}"
                );
            }
        }
        w += 25.0;
    }
}

// ---- Skizzenmodus (Etappe C, Aufgabe 3) -------------------------------------------------------

fn sketch_rects(w: f32, h: f32, read_only: bool) -> ShellRects {
    layout_in_mode(
        window(w, h),
        ShellMode::Sketch,
        &ShellState::default(),
        (600.0, 431.0),
        360.0,
        200.0,
        read_only,
    )
}

/// Alle Felder des Skizzenmodus mit Namen (ohne das leere linke Rechteck).
fn sketch_named(r: &ShellRects) -> Vec<(&'static str, Rect)> {
    let mut v = vec![
        ("top", r.top),
        ("strip", r.strip.expect("Leiste im Skizzenmodus")),
        ("right", r.right),
        ("bottom", r.bottom),
        ("status", r.status),
        ("free", r.free),
    ];
    if let Some(b) = r.banner {
        v.push(("banner", b));
    }
    v
}

// (a)
#[test]
fn sketch_mode_rects_do_not_overlap_at_five_sizes() {
    for (w, h) in [
        (800.0, 600.0),
        (1100.0, 700.0),
        (1400.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
    ] {
        let content = window(w, h);
        for read_only in [false, true] {
            let r = sketch_rects(w, h, read_only);
            assert!(r.tool.is_none(), "Werkzeugfeld nur in der Zeichnung");
            let all = sketch_named(&r);
            for (name, rect) in &all {
                assert!(
                    content.contains_rect(*rect),
                    "{name} liegt bei {w}x{h} außerhalb: {rect:?}"
                );
                assert!(rect.width() > 0.0 && rect.height() > 0.0, "{name} leer");
            }
            for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    assert!(
                        !overlaps(all[i].1, all[j].1),
                        "{} und {} überlappen bei {w}x{h} (schreibgeschützt {read_only}): {:?} / {:?}",
                        all[i].0,
                        all[j].0,
                        all[i].1,
                        all[j].1
                    );
                }
            }
            // Die Leiste steht links am Rand und senkrecht mittig zur freien Fläche.
            let strip = r.strip.unwrap();
            assert_eq!(strip.left(), SHELL_INSET);
            assert_eq!(strip.width(), STRIP_W);
            assert!((strip.center().y - r.free.center().y).abs() < 0.01);
            // Der Würfel liegt in der freien Fläche und damit unter keinem Feld.
            let cube = cube_rect(r.free);
            for (name, rect) in all.iter().filter(|(n, _)| *n != "free") {
                assert!(
                    !overlaps(cube, *rect),
                    "Würfel liegt bei {w}x{h} unter {name}"
                );
            }
        }
    }
}

// (b)
#[test]
fn sketch_free_rect_margins() {
    let r = sketch_rects(1400.0, 900.0, false);
    assert_eq!(r.free.left(), 16.0 + 44.0 + 12.0);
    assert_eq!(r.free.left(), 72.0);
    assert_eq!(r.free.right(), 1400.0 - 278.0);
    assert_eq!(r.free.top(), 76.0);
    assert_eq!(r.free.bottom(), 900.0 - 84.0);
    assert_eq!(
        r.right.width(),
        250.0,
        "Skizzenfeld so breit wie die Spalte"
    );
    // Unten und Status wie im Modell.
    let model = layout_in_mode(
        window(1400.0, 900.0),
        ShellMode::Model,
        &ShellState::default(),
        (600.0, 431.0),
        360.0,
        200.0,
        false,
    );
    assert_eq!(r.bottom, model.bottom);
    assert_eq!(r.status, model.status);
    assert_eq!(r.right, model.right);
    assert_eq!(r.top, model.top);
    assert!(model.strip.is_none() && model.tool.is_none());
    assert_eq!(model.free.left(), 278.0);
}

// (c)
#[test]
fn strip_height_is_limited_to_free_height() {
    // Genug Platz: 9 Knöpfe zu 36, 8 Abstände zu 4, Rand 2 × 4.
    assert_eq!(STRIP_H, 9.0 * 36.0 + 8.0 * 4.0 + 8.0);
    let r = sketch_rects(1400.0, 900.0, false);
    assert_eq!(r.strip.unwrap().height(), STRIP_H);
    // Niedrige Fenster: höchstens so hoch wie die freie Fläche.
    for h in [300.0, 400.0, 480.0, 520.0] {
        for read_only in [false, true] {
            let r = sketch_rects(800.0, h, read_only);
            let strip = r.strip.unwrap();
            assert!(
                strip.height() <= r.free.height() + 0.01,
                "{h}: Leiste {strip:?} höher als die freie Fläche {:?}",
                r.free
            );
            assert!(strip.top() >= r.free.top() - 0.01 && strip.bottom() <= r.free.bottom() + 0.01);
        }
    }
}

// ---- Zeichnungsmodus (Etappe C, Aufgabe 4) ----------------------------------------------------

fn drawing_rects(w: f32, h: f32, read_only: bool) -> ShellRects {
    layout_in_mode(
        window(w, h),
        ShellMode::Drawing,
        &ShellState::default(),
        (600.0, 431.0),
        f32::INFINITY,
        200.0,
        read_only,
    )
}

/// Alle Felder des Zeichnungsmodus mit Namen und die freie Fläche.
fn drawing_named(r: &ShellRects) -> Vec<(&'static str, Rect)> {
    let mut v = vec![
        ("top", r.top),
        ("left", r.left),
        ("tool", r.tool.expect("Werkzeugfeld im Zeichnungsmodus")),
        ("right", r.right),
        ("bottom", r.bottom),
        ("status", r.status),
        ("free", r.free),
    ];
    if let Some(b) = r.banner {
        v.push(("banner", b));
    }
    v
}

// (a)
#[test]
fn drawing_mode_rects_do_not_overlap_at_five_sizes() {
    for (w, h) in [
        (800.0, 600.0),
        (1100.0, 700.0),
        (1400.0, 900.0),
        (1920.0, 1080.0),
        (2560.0, 1440.0),
    ] {
        let content = window(w, h);
        for read_only in [false, true] {
            let r = drawing_rects(w, h, read_only);
            assert!(r.strip.is_none(), "keine Skizzenleiste in der Zeichnung");
            let all = drawing_named(&r);
            for (name, rect) in &all {
                assert!(
                    content.contains_rect(*rect),
                    "{name} liegt bei {w}x{h} außerhalb: {rect:?}"
                );
                assert!(rect.width() > 0.0 && rect.height() > 0.0, "{name} leer");
            }
            for i in 0..all.len() {
                for j in (i + 1)..all.len() {
                    assert!(
                        !overlaps(all[i].1, all[j].1),
                        "{} und {} überlappen bei {w}x{h} (schreibgeschützt {read_only}): {:?} / {:?}",
                        all[i].0,
                        all[j].0,
                        all[i].1,
                        all[j].1
                    );
                }
            }
            // Werkzeugfeld: mittig, direkt unter dem oberen Feld (bzw. dem Hinweis), zwei Zeilen
            // hoch, so breit wie zwischen den Spalten Platz ist.
            let tool = r.tool.unwrap();
            assert_eq!(tool.height(), TOOL_H);
            assert!((tool.center().x - w / 2.0).abs() < 0.01);
            let above = r.banner.map_or(r.top.bottom(), |b| b.bottom());
            assert_eq!(tool.top(), above + GAP, "{w}x{h}");
            assert!(tool.left() >= r.left.right() + GAP - 0.01);
            assert!(tool.right() <= r.right.left() - GAP + 0.01);
            assert_eq!(r.free.top(), tool.bottom() + GAP);
            // Hinweisfeld unten so hoch wie das Befehlsfeld.
            assert_eq!(r.bottom.height(), BOTTOM_H);
        }
    }
}

// (b)
#[test]
fn drawing_free_rect_margins() {
    let r = drawing_rects(1400.0, 900.0, false);
    assert_eq!(r.free.left(), 278.0);
    assert_eq!(r.free.right(), 1400.0 - 278.0);
    assert_eq!(r.free.top(), 16.0 + 48.0 + 12.0 + 76.0 + 12.0);
    assert_eq!(r.free.top(), 164.0);
    assert_eq!(r.free.bottom(), 900.0 - 84.0);
    assert_eq!(
        r.right.width(),
        250.0,
        "Ansichtsfeld so breit wie die Spalte"
    );
    // Linke Spalte, oberes Feld und rechte Spalte wie im Modell.
    let model = layout_in_mode(
        window(1400.0, 900.0),
        ShellMode::Model,
        &ShellState::default(),
        (600.0, 431.0),
        f32::INFINITY,
        200.0,
        false,
    );
    assert_eq!(r.left, model.left);
    assert_eq!(r.right, model.right);
    assert_eq!(r.top, model.top);
    assert_eq!(r.bottom, model.bottom);
    assert_eq!(r.status, model.status);
    let tool = r.tool.unwrap();
    assert_eq!(
        tool,
        Rect::from_min_max(pos2(278.0, 76.0), pos2(1122.0, 152.0))
    );
    // Schreibgeschützt: der Hinweis steht unter dem oberen Feld, das Werkzeugfeld darunter.
    let ro = drawing_rects(1400.0, 900.0, true);
    let banner = ro.banner.unwrap();
    assert_eq!(banner.top(), SHELL_INSET + TOP_H + GAP);
    assert_eq!(ro.tool.unwrap().top(), banner.bottom() + GAP);
    assert_eq!(ro.free.top(), 164.0 + BANNER_H + GAP);
    assert_eq!(ro.free.bottom(), 900.0 - 84.0);
}

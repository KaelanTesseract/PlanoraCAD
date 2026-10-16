//! Geometrie und Zustand der schwebenden Felder (reine Funktionen, ohne egui-Bild).

use egui::{Rect, pos2, vec2};
use planora_app::shell::{GAP, RightTab, SHELL_INSET, ShellRects, ShellState, layout};

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
        assert!(r.left.width() >= 160.0 || w < 400.0);
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

//! Feld „Projekte“ im linken Feld: Mit offener Eingabe „Projektname“ (nach „Neues Projekt“)
//! oder „Projektordner“ (nach „Wurzel wählen …“) bleibt alles in der Karte. Das Textfeld nimmt
//! die freie Breite, die Knöpfe passen daneben oder stehen in einer zweiten Zeile, und die Texte
//! im Feature-Baum und bei den Zeichnungen darunter brechen weiter in der Karte um. Geprüft bei
//! 1400 × 900, 1100 × 700 und 800 × 600, dunkel und hell. Ohne Fenster: Rohereignisse je Bild
//! gegen einen egui-Kontext.

mod common;

use common::{Screen, Seen};
use egui::{LayerId, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::workspace::Workspace;
use planora_app::{shell_panels, theme};
use planora_project::config::Config;
use planora_project::library::Library;

const SIZES: [Vec2; 3] = [vec2(1400.0, 900.0), vec2(1100.0, 700.0), vec2(800.0, 600.0)];

fn app(dark: bool) -> PlanoraApp {
    let root = common::temp_root("planora-pwidth");
    let ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let mut app = PlanoraApp::for_tests(ws);
    app.ws_mut().set_dark(dark);
    assert!(
        app.shell_mut().projects_visible,
        "Projekte sind eingeblendet"
    );
    app
}

fn screen(size: Vec2, dark: bool) -> Screen {
    let s = Screen::new(size.x, size.y);
    theme::apply(&s.ctx, dark);
    s
}

/// Rechteck der linken Karte.
fn card(c: &egui::Context) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == "links")
        .map(|(_, r)| r)
        .expect("linkes Feld")
}

/// Bedienbare Elemente (Klick oder Fokus) der Ebene der linken Karte im letzten Bild.
fn card_widgets(c: &egui::Context, card: Rect) -> Vec<egui::WidgetRect> {
    // Erst die Ebenen lesen, dann den Speicher (beides zugleich sperrt den Kontext).
    let layers: Vec<LayerId> = c.viewport(|v| v.prev_pass.widgets.layer_ids().collect());
    let layer = layers
        .into_iter()
        .find(|l| l.order == egui::Order::Middle && c.memory(|m| m.area_rect(l.id)) == Some(card))
        .expect("Ebene der linken Karte");
    c.viewport(|v| {
        v.prev_pass
            .widgets
            .get_layer(layer)
            .cloned()
            .collect::<Vec<_>>()
    })
    .into_iter()
    .filter(|w| w.sense.senses_click() || w.sense.is_focusable())
    .collect()
}

/// Klickt den Knopf mit der Aufschrift `label` in der linken Karte.
fn click_label(s: &mut Screen, app: &mut PlanoraApp, label: &str) {
    let mut f = |c: &egui::Context| {
        shell_panels::show_fields(c, app);
    };
    let seen = s.settle(3, &mut f);
    let at = common::find(&seen, label)
        .unwrap_or_else(|| panic!("„{label}“ fehlt"))
        .rect
        .center();
    s.click(at, &mut f);
}

/// Prüft: Karte genau `PANEL_WIDTH` breit, jedes bedienbare Element (soweit senkrecht sichtbar)
/// waagerecht ganz in der Karte, jeder Text der Karte endet vor ihrem rechten Rand.
fn assert_inside(s: &mut Screen, app: &mut PlanoraApp, at: &str, want: &[&str]) {
    let seen: Vec<Seen> = s.settle(3, &mut |c| {
        shell_panels::show_fields(c, app);
    });
    let card = card(&s.ctx);
    assert!(
        card.width() <= theme::PANEL_WIDTH + 0.5,
        "{at}: Karte {card:?} ist {} breit",
        card.width()
    );
    for w in want {
        assert!(
            seen.iter()
                .any(|t| t.text == *w && card.contains(t.rect.center())),
            "{at}: „{w}“ fehlt in der Karte"
        );
    }
    let widgets = card_widgets(&s.ctx, card);
    assert!(widgets.len() >= 4, "{at}: zu wenige Elemente {widgets:?}");
    for w in &widgets {
        if w.rect.bottom() <= card.top() || w.rect.top() >= card.bottom() {
            continue; // weggerollt
        }
        assert!(
            w.rect.left() >= card.left() - 0.5 && w.rect.right() <= card.right() + 0.5,
            "{at}: Element {:?} {:?} ragt aus der Karte {card:?}",
            w.id,
            w.rect
        );
    }
    // Ausgenommen ist Text, den ein Element der Karte selbst auf sein Rechteck beschneidet (ein
    // langer Pfad im Textfeld); das Element selbst liegt nach der Prüfung oben in der Karte.
    // (Die Karte selbst ist auch klickbar und zählt hier nicht.)
    let own_clip = |clip: Rect| {
        widgets
            .iter()
            .any(|w| w.rect.width() < card.width() - 1.0 && w.rect.expand(0.5).contains_rect(clip))
    };
    for t in seen.iter().filter(|t| {
        !own_clip(t.clip)
            && t.clip.intersects(t.rect)
            && t.rect.left() >= card.left() - 0.5
            && t.rect.left() < card.right()
            && t.rect.center().y > card.top()
            && t.rect.center().y < card.bottom()
    }) {
        assert!(
            t.rect.right() <= card.right() + 0.5,
            "{at}: „{}“ {:?} ragt über den rechten Rand der Karte {card:?}",
            t.text,
            t.rect
        );
    }
}

#[test]
fn the_project_name_field_fits_into_the_card() {
    for dark in [true, false] {
        for size in SIZES {
            let at = format!("Projektname, {size:?}, dunkel {dark}");
            let mut s = screen(size, dark);
            let mut a = app(dark);
            click_label(&mut s, &mut a, "Neues Projekt");
            assert_inside(
                &mut s,
                &mut a,
                &at,
                &["Projektname", "Anlegen", "Abbrechen"],
            );
        }
    }
}

#[test]
fn the_root_field_fits_into_the_card() {
    for dark in [true, false] {
        for size in SIZES {
            let at = format!("Wurzel, {size:?}, dunkel {dark}");
            let mut s = screen(size, dark);
            let mut a = app(dark);
            click_label(&mut s, &mut a, "Wurzel wählen …");
            assert_inside(
                &mut s,
                &mut a,
                &at,
                &["Übernehmen", "Abbrechen", "Aktualisieren"],
            );
        }
    }
}

#[test]
fn both_fields_open_still_fit_and_the_tree_text_wraps_inside() {
    for size in SIZES {
        let at = format!("beide Felder, {size:?}");
        let mut s = screen(size, true);
        let mut a = app(true);
        click_label(&mut s, &mut a, "Neues Projekt");
        click_label(&mut s, &mut a, "Wurzel wählen …");
        assert_inside(
            &mut s,
            &mut a,
            &at,
            &["Projektname", "Anlegen", "Übernehmen", "Feature-Baum"],
        );
        // Der Hinweis im leeren Feature-Baum wird nicht gekürzt, er bricht in der Karte um.
        let seen = s.settle(2, &mut |c| {
            shell_panels::show_fields(c, &mut a);
        });
        let card = card(&s.ctx);
        let hint: Vec<&Seen> = seen
            .iter()
            .filter(|t| t.text.starts_with("Noch keine Features") && card.contains(t.rect.min))
            .collect();
        assert!(!hint.is_empty(), "{at}: Hinweis im Feature-Baum fehlt");
        for t in hint {
            assert!(!t.text.contains('…'), "{at}: Hinweis gekürzt: {}", t.text);
            assert!(t.rect.right() <= card.right() + 0.5, "{at}: {:?}", t.rect);
        }
    }
}

/// Mit offenem Feld lässt sich ein Projekt anlegen (Text eingeben, „Anlegen“).
#[test]
fn a_project_can_still_be_created_from_the_field() {
    let mut s = screen(vec2(1100.0, 700.0), true);
    let mut a = app(true);
    click_label(&mut s, &mut a, "Neues Projekt");
    let mut f = |c: &egui::Context| {
        shell_panels::show_fields(c, &mut a);
    };
    let seen = s.settle(2, &mut f);
    let field = common::find(&seen, "Projektname").unwrap().rect.center();
    s.click(field, &mut f);
    s.frame(vec![egui::Event::Text("Gehäuse".into())], &mut f);
    let seen = s.settle(2, &mut f);
    let create = common::find(&seen, "Anlegen").unwrap().rect.center();
    s.click(create, &mut f);
    let seen = s.settle(3, &mut f);
    assert!(common::find(&seen, "Gehäuse").is_some(), "Projekt im Baum");
    assert!(common::find(&seen, "Anlegen").is_none(), "Feld geschlossen");
}

/// Die Zeile „Projektname“: Das Feld reicht bis an den inneren rechten Rand der Karte (nimmt die
/// freie Breite), „Anlegen“ und „Abbrechen“ stehen in einer zweiten Zeile darunter.
fn assert_name_row(s: &mut Screen, app: &mut PlanoraApp, at: &str, dark: bool) {
    let seen: Vec<Seen> = s.settle(3, &mut |c| {
        shell_panels::show_fields(c, app);
    });
    let card = card(&s.ctx);
    let frame = theme::panel_frame(dark);
    let inner_right = card.right() - f32::from(frame.inner_margin.right) - frame.stroke.width;
    let hint = common::find(&seen, "Projektname")
        .unwrap_or_else(|| panic!("{at}: Feld fehlt"))
        .rect
        .center();
    let field = card_widgets(&s.ctx, card)
        .into_iter()
        .filter(|w| w.rect.contains(hint) && w.rect.width() < card.width() - 1.0)
        .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
        .unwrap_or_else(|| panic!("{at}: Element des Felds fehlt"))
        .rect;
    assert!(
        (inner_right - field.right()).abs() <= 2.0,
        "{at}: Feld {field:?} endet nicht am inneren Rand {inner_right} der Karte {card:?}"
    );
    for label in ["Anlegen", "Abbrechen"] {
        let r = common::find(&seen, label)
            .unwrap_or_else(|| panic!("{at}: „{label}“ fehlt"))
            .rect;
        assert!(
            r.top() >= field.bottom(),
            "{at}: „{label}“ {r:?} steht nicht unter dem Feld {field:?}"
        );
    }
}

#[test]
fn the_project_name_field_takes_the_free_width_and_the_buttons_wrap() {
    for dark in [true, false] {
        for size in SIZES {
            let at = format!("Zeile Projektname, {size:?}, dunkel {dark}");
            let mut s = screen(size, dark);
            let mut a = app(dark);
            click_label(&mut s, &mut a, "Neues Projekt");
            assert_name_row(&mut s, &mut a, &at, dark);
        }
    }
}

/// Auch in einem breiteren Bereich (breiter als die Vorgabebreite eines Textfelds von egui)
/// nimmt das Feld die ganze freie Breite.
#[test]
fn the_project_name_field_fills_a_wide_area_too() {
    let root = common::temp_root("planora-pwide");
    let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let mut st = planora_app::projects_panel::ProjectsUi::default();
    let area = Rect::from_min_size(egui::pos2(20.0, 20.0), vec2(600.0, 400.0));
    let mut s = Screen::new(800.0, 600.0);
    let mut f = |c: &egui::Context| {
        egui::Area::new(egui::Id::new("breit"))
            .fixed_pos(area.min)
            .show(c, |ui| {
                ui.set_width(area.width());
                ui.set_max_width(area.width());
                planora_app::projects_panel::show(ui, &mut ws, &mut st);
            });
    };
    let seen = s.settle(3, &mut f);
    let at = common::find(&seen, "Neues Projekt").unwrap().rect.center();
    s.click(at, &mut f);
    let seen = s.settle(3, &mut f);
    let hint = common::find(&seen, "Projektname")
        .expect("Feld")
        .rect
        .center();
    let layers: Vec<LayerId> = s
        .ctx
        .viewport(|v| v.prev_pass.widgets.layer_ids().collect());
    let field = layers
        .into_iter()
        .flat_map(|l| {
            s.ctx.viewport(|v| {
                v.prev_pass
                    .widgets
                    .get_layer(l)
                    .cloned()
                    .collect::<Vec<_>>()
            })
        })
        .filter(|w| w.sense.is_focusable() && w.rect.contains(hint))
        .min_by(|a, b| a.rect.area().total_cmp(&b.rect.area()))
        .expect("Element des Felds")
        .rect;
    assert!(
        (area.right() - field.right()).abs() <= 2.0,
        "Feld {field:?} endet nicht am rechten Rand {} des Bereichs",
        area.right()
    );
    let create = common::find(&seen, "Anlegen").expect("Anlegen").rect;
    assert!(create.top() >= field.bottom(), "{create:?} unter {field:?}");
}

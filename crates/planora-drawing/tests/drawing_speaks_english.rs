//! planora-drawing auf Englisch (Sprachplan, Task 14): alle Fehlermeldungen und die Texte, die
//! beim Zeichnen und Exportieren auf das Blatt oder in die Datei kommen (Schriftfeld,
//! Änderungstabelle, Stückliste, Schnitt- und Ansichtsnamen, Ebenen und Gruppen, Maßzahlen),
//! unter `with_lang(&english(), …)`. Unter Deutsch bleiben die bisherigen Texte (die goldenen
//! Dateien der übrigen Tests sind unverändert). Kein Test ruft `set_global` (Ruling 4).

use planora_drawing::annotate::ResolvedDims;
use planora_drawing::dimension::{Anchor, DimError, DimId, DimKind, DimSpec, Measured, Tolerance};
use planora_drawing::isect::zero_width;
use planora_drawing::model::DrawingKind;
use planora_drawing::parts::{
    RowOverride, parts_doc_type, parts_head, parts_sheet_name, validate_override,
};
use planora_drawing::pdf::{no_pages, page_too_large, pdf_char_message, too_many_pages};
use planora_drawing::section::{SectionPlane, compute_section};
use planora_drawing::section_marks::{add_section_marks, section_marks, section_view_name};
use planora_drawing::sheet::{SheetModel, build_sheet_with};
use planora_drawing::sheet_frame::{PartsSpace, TITLE_CELLS, sheet_layout_with};
use planora_drawing::sheet_info::{
    FieldValue, NotesValue, ProjectInfo, SheetContext, SheetInfo, Stamp, default_notes, main_scale,
    resolve_texts, too_many_revisions, validate_project_info, validate_sheet_info,
};
use planora_drawing::sheet_marks::sheet_frame;
use planora_drawing::silhouette::{hidden_message, pole_message};
use planora_drawing::{
    DrawBody, DrawError, Drawing, DrawingFormat, DrawingId, HatchSpec, Paper, Placement,
    Projection, Scale, Sheet, TitleField, ViewId, ViewKind, ViewLines, compute_view, dxf_string,
    sections_ok_on, svg_string,
};
use planora_exchange::Progress;
use planora_i18n::{english, german, with_lang};
use planora_kernel::{AnalyticKernel, Kernel, Solid};
use planora_math::{DVec2, DVec3};
use std::collections::BTreeMap;

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

/// Kein deutscher Buchstabe, kein deutsches Anführungszeichen und nichts, das wie eine
/// Schlüssel-Id aussieht.
fn assert_english(what: &str, text: &str) {
    assert!(!text.trim().is_empty(), "{what}: leer");
    assert!(
        !text.chars().any(|c| "äöüßÄÖÜ„".contains(c)),
        "{what}: deutsch: {text}"
    );
    let looks_like_id = text.contains('-')
        && text.split('-').all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
        && text.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    assert!(!looks_like_id, "{what}: sieht aus wie eine Id: {text}");
}

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-30".into(),
        user: "dennis".into(),
    }
}

/// Alle Meldungen der Crate, die sich ohne Geometrie bauen lassen.
fn all_messages() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |what: &str, t: String| out.push((what.to_string(), t));
    for (n, e) in [
        ("NoBodies", DrawError::NoBodies),
        ("MeshBodiesOnly", DrawError::MeshBodiesOnly),
        (
            "AmbiguousVisibility",
            DrawError::AmbiguousVisibility("Box 1, edge 3".into()),
        ),
        ("TooComplex", DrawError::TooComplex(123)),
        ("SectionMisses", DrawError::SectionMisses),
        ("SectionFailed", DrawError::SectionFailed("Box 2".into())),
        ("NonFinite", DrawError::NonFinite),
        ("Cancelled", DrawError::Cancelled),
        ("Io", DrawError::Io(std::io::Error::from_raw_os_error(32))),
        ("InvalidBody", DrawError::InvalidBody(zero_width())),
        ("Unsupported", DrawError::Unsupported(pole_message())),
        (
            "Unsupported hidden",
            DrawError::Unsupported(hidden_message()),
        ),
        ("Unsupported pdf", DrawError::Unsupported(no_pages())),
        (
            "Unsupported pdf 2",
            DrawError::Unsupported(too_many_pages()),
        ),
        (
            "Unsupported pdf 3",
            DrawError::Unsupported(page_too_large()),
        ),
        (
            "Unsupported char",
            DrawError::Unsupported(pdf_char_message('€')),
        ),
        (
            "Unsupported space",
            DrawError::Unsupported(pdf_char_message(' ')),
        ),
    ] {
        add(n, e.to_string());
    }
    for (n, e) in [
        ("NotAcross", DimError::NotAcross),
        ("NotVisible", DimError::NotVisible),
        ("Ambiguous", DimError::Ambiguous),
        ("Lost", DimError::Lost("The edge is gone.".into())),
        ("NotAllowedInIsometric", DimError::NotAllowedInIsometric),
        ("TooMany", DimError::TooMany),
        ("Invalid", DimError::Invalid("x".into())),
    ] {
        add(n, e.to_string());
    }
    for k in ViewKind::ALL {
        add("ViewKind", k.label());
    }
    for f in TitleField::ALL {
        add("TitleField", f.label());
    }
    for c in &TITLE_CELLS {
        add("TitleCell", c.label.text());
    }
    for h in parts_head() {
        add("PartsHead", h);
    }
    add("PartsDocType", parts_doc_type());
    add("PartsSheet 1", parts_sheet_name(1));
    add("PartsSheet 2", parts_sheet_name(2));
    add("SectionName", section_view_name('A'));
    add("TooManyRevisions", too_many_revisions());
    for n in default_notes("ISO 2768-m") {
        add("Note", n);
    }
    for f in [DrawingFormat::Svg, DrawingFormat::Dxf, DrawingFormat::Pdf] {
        add("Format", f.label());
    }

    // Prüfungen der Blattangaben: Steuerzeichen und Überlänge in jedem Feld.
    let long = "x".repeat(200);
    let mut sheet = SheetInfo::new(&stamp());
    sheet.created = "a\u{7}".into();
    add("created", validate_sheet_info(&sheet).unwrap_err());
    let fields: [fn(&mut SheetInfo) -> &mut FieldValue; 11] = [
        |i| &mut i.number,
        |i| &mut i.title,
        |i| &mut i.subtitle,
        |i| &mut i.doc_type,
        |i| &mut i.status,
        |i| &mut i.issued,
        |i| &mut i.revision,
        |i| &mut i.sheet_of,
        |i| &mut i.format,
        |i| &mut i.scale,
        |i| &mut i.weight,
    ];
    for f in fields {
        let mut i = SheetInfo::new(&stamp());
        *f(&mut i) = FieldValue::Text(long.clone());
        add("sheet field", validate_sheet_info(&i).unwrap_err());
    }
    let mut i = SheetInfo::new(&stamp());
    i.notes = NotesValue::Lines(vec!["n".into(); 7]);
    add("notes", validate_sheet_info(&i).unwrap_err());
    i.notes = NotesValue::Lines(vec![long.clone()]);
    add("note", validate_sheet_info(&i).unwrap_err());
    let mut i = SheetInfo::new(&stamp());
    i.add_revision("2026-10-30", "dg").unwrap();
    i.revisions[0].description = long.clone();
    add("revision", validate_sheet_info(&i).unwrap_err());
    for _ in 0..30 {
        let _ = i.add_revision("2026-10-30", "dg");
    }
    add("add_revision", i.add_revision("d", "n").unwrap_err());
    let mut p = ProjectInfo::new(&stamp());
    p.owner = FieldValue::Text(long.clone());
    add("owner", validate_project_info(&p).unwrap_err());
    let mut p = ProjectInfo::new(&stamp());
    p.tolerance = FieldValue::Text("\u{1}".into());
    add("tolerance", validate_project_info(&p).unwrap_err());
    let o = RowOverride {
        note: FieldValue::Text(long.clone()),
        ..RowOverride::default()
    };
    add("override", validate_override(&o).unwrap_err());

    // Schnitte und Schraffur.
    let mut d = Drawing::new(DrawingId(1), "T".into(), A3_QUER, Projection::First);
    let iso = d.add_view(
        ViewKind::Isometric,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    add(
        "add_section iso",
        d.add_section(iso, ViewKind::Top, 0.0, 50.0).unwrap_err(),
    );
    let front = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 200.0, y: 100.0 },
        None,
    );
    let sec = d.add_section(front, ViewKind::Top, 0.0, 50.0).unwrap();
    let hatch = |d: &mut Drawing, spacing: f64| {
        let v = d.views.iter_mut().find(|v| v.id == sec).unwrap();
        v.section.as_mut().unwrap().hatch = HatchSpec {
            spacing,
            ..HatchSpec::default()
        };
    };
    hatch(&mut d, 500.0);
    add("hatch max", sections_ok_on(&d, A3_QUER).unwrap_err());
    hatch(&mut d, 0.01);
    add("hatch min", sections_ok_on(&d, A3_QUER).unwrap_err());
    for _ in 0..20 {
        if let Err(e) = d.add_section(front, ViewKind::Top, 0.0, 50.0) {
            add("sections max", e);
            break;
        }
    }
    out
}

#[test]
fn all_messages_speak_english() {
    let en = with_lang(&english(), all_messages);
    let de = with_lang(&german(), all_messages);
    assert_eq!(en.len(), de.len());
    assert!(en.len() >= 75, "zu wenige Texte: {}", en.len());
    for (what, t) in &en {
        assert_english(what, t);
    }
    // Gleich lauten nur Wörter, die in beiden Sprachen gleich sind.
    const SAME_IN_BOTH: [&str; 5] = ["Format", "Name", "Rev.", "Material", "Revision"];
    for ((what, a), (_, b)) in en.iter().zip(&de) {
        assert!(
            a != b || SAME_IN_BOTH.contains(&a.as_str()) || a.starts_with("Format"),
            "{what}: in beiden Sprachen gleich: {a}"
        );
    }
    // Stichproben unter Deutsch (Wortlaut wie vorher).
    let de_text = |what: &str| {
        de.iter()
            .find(|(w, _)| w == what)
            .map(|(_, t)| t.clone())
            .unwrap()
    };
    assert_eq!(de_text("PartsSheet 2"), "Stückliste (2)");
    assert_eq!(de_text("SectionName"), "Schnitt A\u{2013}A");
    assert_eq!(
        de_text("Unsupported char"),
        "Das wird nicht unterstützt: Das Zeichen „€“ lässt sich in einer PDF nicht ausgeben."
    );
    assert_eq!(
        de_text("Io"),
        format!(
            "Die Datei lässt sich nicht schreiben: {} Ist die Datei in einem anderen Programm \
             geöffnet? Bitte dort schließen.",
            std::io::Error::from_raw_os_error(32)
        )
    );
    assert_eq!(
        de_text("hatch min"),
        "Der Schraffurabstand von „Schnitt A\u{2013}A“ wäre auf diesem Blatt kleiner als 0,5 mm."
    );
    let en_text = |what: &str| {
        en.iter()
            .find(|(w, _)| w == what)
            .map(|(_, t)| t.clone())
            .unwrap()
    };
    assert_eq!(en_text("PartsSheet 2"), "Parts list (2)");
    assert_eq!(en_text("SectionName"), "Section A\u{2013}A");
    assert_eq!(
        en_text("Unsupported char"),
        "This is not supported: The character \"€\" cannot be written to a PDF."
    );
}

// ---- Blatt und Dateien ----------------------------------------------------------------------

fn bar() -> Solid {
    AnalyticKernel::default()
        .make_box(DVec3::new(38.8, 20.0, 10.0))
        .unwrap()
}

/// Ein Blatt A3 quer mit Rahmen und Schriftfeld (Vorgaben), einer Änderung, zwei Zeilen
/// Stückliste, der Vorderansicht mit einem waagerechten Maß 38,8 und dem Schnitt A–A. Alle Texte
/// in der aktuellen Sprache des Threads.
fn sheet_model(kind: DrawingKind) -> SheetModel {
    let solid = bar();
    let bodies = [DrawBody {
        name: "Bar".into(),
        solid: &solid,
    }];
    let mut d = Drawing::new(DrawingId(1), "Halter".into(), A3_QUER, Projection::First);
    d.kind = kind;
    let one = Scale { num: 1, den: 1 };
    let front = d.add_view(
        ViewKind::Front,
        one,
        Placement::Free { x: 100.0, y: 180.0 },
        None,
    );
    let p = Progress::new();
    let front_lines = compute_view(&bodies, ViewKind::Front, &p).unwrap();
    let mut results: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(front, &front_lines)]);
    let sec = d.add_section(front, ViewKind::Top, 5.0, 60.0).unwrap();
    let plane = SectionPlane {
        kind: ViewKind::Top,
        offset: 5.0,
    };
    let sec_lines = compute_section(&bodies, &plane, compute_view, &p).unwrap();
    results.insert(sec, &sec_lines.lines);

    // Waagerechtes Maß über die ganze Breite (38,8).
    let ((x0, _), (x1, y1)) = front_lines.bbox;
    let spec = DimSpec {
        id: DimId(1),
        view: front,
        kind: DimKind::Horizontal,
        refs: Vec::new(),
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    };
    let (a, b) = (DVec2::new(x0, y1), DVec2::new(x1, y1));
    let m = Measured {
        value: x1 - x0,
        anchor: Anchor::Linear {
            a,
            b,
            dir: DVec2::new(1.0, 0.0),
            a_span: (a, a),
            b_span: (b, b),
        },
        prefix: None,
    };
    let mut dims = ResolvedDims::default();
    dims.by_view.insert(front, vec![(spec, Ok(m))]);
    let mut model = build_sheet_with(&d, &results, &dims);
    let sections = BTreeMap::from([(sec, &sec_lines.data)]);
    let (list, errs) = section_marks(&d, &results, &sections);
    assert!(errs.is_empty(), "{errs:?}");
    add_section_marks(&mut model, &list);

    // Rahmen mit Schriftfeld, Änderungstabelle, Hinweisen und Stückliste.
    let mut info = SheetInfo::new(&stamp());
    info.add_revision("2026-10-30", "dennis").unwrap();
    info.revisions[0].description = "First issue".into();
    d.info = Some(info);
    let project = ProjectInfo::new(&stamp());
    let ctx = SheetContext {
        doc_name: "Halter".into(),
        index: 1,
        count: 1,
    };
    let mut texts = resolve_texts(&d, Some(&project), &ctx, main_scale(&d));
    texts.parts = vec![[
        "1".into(),
        "Bar".into(),
        "1".into(),
        "S235".into(),
        String::new(),
    ]];
    let space = match kind {
        DrawingKind::Views => PartsSpace::OnSheet { rows: 1 },
        DrawingKind::PartsList { .. } => PartsSpace::ListPage,
    };
    let layout = sheet_layout_with(A3_QUER, 1, texts.notes.len(), space);
    model.frame = Some(sheet_frame(&layout, &texts, d.projection));
    model
}

/// Alle Texte eines SVG (Inhalt der `<text>`-Elemente) und Gruppen-Kennungen.
fn svg_texts(svg: &str) -> (Vec<String>, Vec<String>) {
    let mut texts = Vec::new();
    let mut ids = Vec::new();
    for part in svg.split("<text").skip(1) {
        let body = part.split_once('>').map(|x| x.1).unwrap_or("");
        let t = body.split("</text>").next().unwrap_or("");
        texts.push(t.replace("&amp;", "&"));
    }
    for part in svg.split("<g id=\"").skip(1) {
        ids.push(part.split('"').next().unwrap_or("").to_string());
    }
    (texts, ids)
}

#[test]
fn svg_sheet_speaks_the_current_language() {
    let en = with_lang(&english(), || {
        svg_string(&sheet_model(DrawingKind::Views)).unwrap()
    });
    let (texts, ids) = svg_texts(&en);
    for t in &texts {
        assert_english("SVG-Text", t);
    }
    for id in &ids {
        assert_english("SVG-Gruppe", id);
    }
    for must in [
        "38.8",
        "A\u{2013}A (1:1)",
        "Drawing number",
        "Sheet",
        "Language",
        "en",
        "1 of 1",
        "Owner",
        "Document type",
        "Manufacturing drawing",
        "Document status",
        "in progress",
        "Material",
        "Scale",
        "Projection",
        "Rev.",
        "Change",
        "Date",
        "Item",
        "Part name",
        "Quantity",
        "Remark",
        "General tolerances ISO 2768-m",
        "First issue",
    ] {
        assert!(texts.iter().any(|t| t == must), "„{must}“ fehlt: {texts:?}");
    }
    for must in [
        "Section A\u{2013}A",
        "Front",
        "Front Dimensions",
        "Section A\u{2013}A Hatching",
        "Sheet frame",
    ] {
        assert!(
            ids.iter().any(|t| t == must),
            "Gruppe „{must}“ fehlt: {ids:?}"
        );
    }

    // Unter Deutsch die bisherigen Texte.
    let de = with_lang(&german(), || {
        svg_string(&sheet_model(DrawingKind::Views)).unwrap()
    });
    let (texts, ids) = svg_texts(&de);
    for must in [
        "38,8",
        "Zeichnungsnummer",
        "Sprache",
        "de",
        "1 von 1",
        "Fertigungszeichnung",
        "in Arbeit",
        "Index",
        "Projektion",
        "Änderung",
        "Pos.",
        "Benennung",
        "Allgemeintoleranzen ISO 2768-m",
        "Oberflächen Ra 3,2 (soweit nicht anders angegeben)",
    ] {
        assert!(texts.iter().any(|t| t == must), "„{must}“ fehlt: {texts:?}");
    }
    for must in ["Schnitt A\u{2013}A", "Vorne Maße", "Blattrahmen"] {
        assert!(
            ids.iter().any(|t| t == must),
            "Gruppe „{must}“ fehlt: {ids:?}"
        );
    }
}

#[test]
fn parts_list_sheet_speaks_the_current_language() {
    let kind = DrawingKind::PartsList { page: 1 };
    let en = with_lang(&english(), || svg_string(&sheet_model(kind)).unwrap());
    let (texts, _) = svg_texts(&en);
    assert!(texts.iter().any(|t| t == "Parts list"), "{texts:?}");
    let de = with_lang(&german(), || svg_string(&sheet_model(kind)).unwrap());
    let (texts, _) = svg_texts(&de);
    assert!(texts.iter().any(|t| t == "Stückliste"), "{texts:?}");
}

#[test]
fn dxf_layers_and_blocks_speak_the_current_language() {
    let en = with_lang(&english(), || {
        dxf_string(&sheet_model(DrawingKind::Views)).unwrap()
    });
    for must in [
        "\r\nVISIBLE\r\n",
        "\r\nDIMENSIONS\r\n",
        "\r\nSHEET_FRAME\r\n",
        "\r\nSECTION_LINE\r\n",
        "\r\nHATCHING\r\n",
        "\r\nFront\r\n",
        "\r\n38.8\r\n",
        "\r\nDrawing number\r\n",
    ] {
        assert!(en.contains(must), "{must:?} fehlt");
    }
    for german_word in ["SICHTBAR", "MASSE", "BLATTRAHMEN", "Vorne", "38,8"] {
        assert!(!en.contains(german_word), "{german_word} im englischen DXF");
    }
    let de = with_lang(&german(), || {
        dxf_string(&sheet_model(DrawingKind::Views)).unwrap()
    });
    for must in [
        "\r\nSICHTBAR\r\n",
        "\r\nMASSE\r\n",
        "\r\nBLATTRAHMEN\r\n",
        "\r\nVorne\r\n",
        "\r\n38,8\r\n",
    ] {
        assert!(de.contains(must), "{must:?} fehlt");
    }
}

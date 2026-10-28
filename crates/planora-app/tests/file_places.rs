//! Dateiauswahl (Teilprojekt Dateiauswahl): Ordner- und Dateisymbole, Schnellzugriffe über der
//! Liste, Favoriten per Rechtsklick. Die Konfiguration wird nur mit `save_to`/`load_from` in
//! eigenen Temp-Ordnern geschrieben und gelesen, nie die echte.

use planora_app::file_chooser::*;
use planora_app::icons::{self, Icon};
use planora_app::places::*;
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_project::config::Config;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-places-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn sep() -> String {
    std::path::MAIN_SEPARATOR.to_string()
}

/// Ein Beispielpfad in Windows-Schreibweise (`D:\Teile`) als Pfad dieses Systems: unter Windows
/// unverändert, sonst `/D:/Teile` (das Laufwerk wird ein gewöhnlicher Ordner). Ohne Windows wäre
/// `\` ein Zeichen im Ordnernamen und `D:\Kunde A\Teile` ein einziger Name.
fn win(s: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(s)
    } else {
        PathBuf::from(format!("/{}", s.replace('\\', "/")))
    }
}

// --- Liste der Schnellzugriffe ----------------------------------------------------------------

#[test]
fn known_folders_present_absent_and_onedrive() {
    let home = temp_dir("home");
    fs::create_dir_all(home.join("Documents")).unwrap();
    fs::create_dir_all(home.join("Downloads")).unwrap();
    // Desktop nur unter OneDrive, Bilder gar nicht.
    fs::create_dir_all(home.join("OneDrive").join("Desktop")).unwrap();
    // Dokumente auch unter OneDrive: der einfache Ordner hat Vorrang.
    fs::create_dir_all(home.join("OneDrive").join("Documents")).unwrap();
    let list = known_places(Some(&home), &|p| p.is_dir());
    let got: Vec<(PlaceKind, &str, PathBuf)> = list
        .iter()
        .map(|p| (p.kind, p.label.as_str(), p.path.clone()))
        .collect();
    assert_eq!(
        got,
        [
            (PlaceKind::Documents, "Dokumente", home.join("Documents")),
            (PlaceKind::Downloads, "Downloads", home.join("Downloads")),
            (
                PlaceKind::Desktop,
                "Desktop",
                home.join("OneDrive").join("Desktop")
            ),
        ]
    );
    assert!(known_places(None, &|_| true).is_empty());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn known_folder_prefers_the_plain_folder() {
    let home = Path::new("H:/Benutzer");
    let all = |_: &Path| true;
    assert_eq!(
        known_folder(home, "Pictures", &all),
        Some(home.join("Pictures"))
    );
    let only_cloud = |p: &Path| p.starts_with(home.join("OneDrive"));
    assert_eq!(
        known_folder(home, "Pictures", &only_cloud),
        Some(home.join("OneDrive").join("Pictures"))
    );
    assert_eq!(known_folder(home, "Pictures", &|_| false), None);
}

#[test]
fn places_are_known_then_project_then_drives_then_favorites() {
    let known = vec![Place {
        kind: PlaceKind::Documents,
        label: "Dokumente".into(),
        path: PathBuf::from("H:/Dokumente"),
    }];
    let drives = [PathBuf::from("C:\\"), PathBuf::from("D:\\")];
    let favs = [PathBuf::from("D:/Teile")];
    let list = build_places(&known, Some(Path::new("D:/Projekte")), &drives, &favs);
    let got: Vec<(PlaceKind, &str)> = list.iter().map(|p| (p.kind, p.label.as_str())).collect();
    assert_eq!(
        got,
        [
            (PlaceKind::Documents, "Dokumente"),
            (PlaceKind::Project, &project_label()),
            (PlaceKind::Drive, "C:"),
            (PlaceKind::Drive, "D:"),
            (PlaceKind::Favorite, "Teile"),
        ]
    );
    assert_eq!(list[1].path, PathBuf::from("D:/Projekte"));
    let without = build_places(&known, None, &[], &[]);
    assert_eq!(without.len(), 1);
}

#[test]
fn every_place_kind_has_its_own_symbol() {
    use PlaceKind::*;
    let kinds = [
        Documents, Downloads, Desktop, Pictures, Project, Drive, Favorite,
    ];
    let mut seen = std::collections::BTreeSet::new();
    for k in kinds {
        assert!(seen.insert(icons::glyph(k.icon())), "{k:?}");
    }
    assert_eq!(Favorite.icon(), Icon::Favorite);
    assert_eq!(Drive.icon(), Icon::Drive);
}

#[test]
fn the_chooser_shows_the_project_root_only_when_it_exists_and_favorites_always() {
    let d = temp_dir("projekt");
    let root = d.join("Projekte");
    fs::create_dir_all(&root).unwrap();
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let missing_fav = d.join("weg");
    let shared = Places {
        favorites: vec![missing_fav.clone()],
        project_root: Some(root.clone()),
        ..Places::default()
    };
    let list = c.places_with(&shared, &[]);
    assert!(
        list.iter()
            .any(|p| p.kind == PlaceKind::Project && p.path == root),
        "{list:?}"
    );
    assert!(
        list.iter()
            .any(|p| p.kind == PlaceKind::Favorite && p.path == missing_fav),
        "fehlende Favoriten bleiben sichtbar: {list:?}"
    );
    let gone = Places {
        project_root: Some(d.join("kein-projekt")),
        ..Places::default()
    };
    assert!(
        !c.places_with(&gone, &[])
            .iter()
            .any(|p| p.kind == PlaceKind::Project)
    );
    let _ = fs::remove_dir_all(&d);
}

// --- Laufwerke im Hintergrund -------------------------------------------------------------------

#[test]
fn drives_come_from_a_background_provider_without_blocking() {
    let (release, wait) = mpsc::channel::<()>();
    let started = Instant::now();
    let scan = DriveScan::start(move |sink| {
        sink.report(PathBuf::from("Z:\\"));
        // Hängt wie ein getrenntes Netzlaufwerk, bis der Test es freigibt.
        let _ = wait.recv_timeout(Duration::from_secs(5));
        sink.report(PathBuf::from("D:\\"));
    });
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "Start wartet nicht auf die Suche"
    );
    assert!(!scan.is_done(), "Suche läuft noch");
    assert!(!scan.known().contains(&PathBuf::from("D:\\")));
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !scan.is_done() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(scan.is_done());
    assert_eq!(
        scan.known(),
        [PathBuf::from("D:\\"), PathBuf::from("Z:\\")],
        "sortiert"
    );
    // In die Liste der Schnellzugriffe.
    let list = build_places(&[], None, &scan.known(), &[]);
    assert_eq!(
        list.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
        ["D:", "Z:"]
    );
}

// --- Hervorhebung ------------------------------------------------------------------------------

#[test]
fn the_place_of_the_current_folder_is_highlighted() {
    let p = Place {
        kind: PlaceKind::Favorite,
        label: "Teile".into(),
        path: PathBuf::from("D:\\Teile"),
    };
    assert!(is_current(&p, Path::new("D:\\Teile")));
    assert!(is_current(&p, Path::new("D:\\Teile\\")));
    assert!(!is_current(&p, Path::new("D:\\Teile\\Innen")));
    assert!(!is_current(&p, Path::new("D:\\Teil")));
    if cfg!(windows) {
        assert!(is_current(&p, Path::new("d:/teile/")));
    }
    // In der Auswahl: genau der Knopf des aktuellen Ordners.
    let d = temp_dir("hervor");
    fs::create_dir_all(d.join("a")).unwrap();
    let mut c = FileChooser::open(d.join("a"), vec!["stl"]);
    let shared = Places {
        favorites: vec![d.join("a"), d.clone()],
        ..Places::default()
    };
    let list = c.places_with(&shared, &[]);
    let hits: Vec<&Path> = list
        .iter()
        .filter(|p| is_current(p, c.dir()))
        .map(|p| p.path.as_path())
        .collect();
    assert_eq!(hits, [d.join("a").as_path()]);
    let _ = fs::remove_dir_all(&d);
}

// --- Favoriten ---------------------------------------------------------------------------------

#[test]
fn favorites_are_deduplicated_and_keep_their_order() {
    let mut f = Vec::new();
    assert_eq!(add_favorite(&mut f, Path::new("D:\\Teile")), Ok(true));
    assert_eq!(add_favorite(&mut f, Path::new("C:\\Ablage")), Ok(true));
    assert_eq!(add_favorite(&mut f, Path::new("D:\\Teile\\")), Ok(false));
    if cfg!(windows) {
        assert_eq!(add_favorite(&mut f, Path::new("d:/teile")), Ok(false));
    }
    assert_eq!(f, [PathBuf::from("D:\\Teile"), PathBuf::from("C:\\Ablage")]);
    assert!(is_favorite(&f, Path::new("C:\\Ablage\\")));
    assert!(remove_favorite(&mut f, Path::new("D:\\Teile\\")));
    assert!(!remove_favorite(&mut f, Path::new("D:\\Teile")));
    assert_eq!(f, [PathBuf::from("C:\\Ablage")]);
}

#[test]
fn at_most_24_favorites() {
    let mut f = Vec::new();
    for i in 0..MAX_FAVORITES {
        assert_eq!(
            add_favorite(&mut f, &PathBuf::from(format!("D:\\F{i}"))),
            Ok(true)
        );
    }
    assert_eq!(MAX_FAVORITES, 24);
    assert_eq!(
        add_favorite(&mut f, Path::new("D:\\Noch einer")),
        Err(favorites_full())
    );
    assert_eq!(f.len(), MAX_FAVORITES);
    // Ein vorhandener ist auch bei voller Liste kein Fehler.
    assert_eq!(add_favorite(&mut f, Path::new("D:\\F0")), Ok(false));
    // Über das Kontextmenü: kurze Meldung in der Auswahl.
    let d = temp_dir("voll");
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let mut shared = Places {
        favorites: f.clone(),
        ..Places::default()
    };
    c.apply_menu(&RowMenu::AddFavorite(d.join("x")), &mut shared);
    assert_eq!(c.error.as_deref(), Some(favorites_full().as_str()));
    assert!(!shared.changed());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn places_mark_changes_only_when_the_list_changes() {
    let mut p = Places::default();
    p.add_favorite(Path::new("D:\\Teile")).unwrap();
    assert_eq!(p.changes, [FavoriteChange::Add(PathBuf::from("D:\\Teile"))]);
    p.add_favorite(Path::new("D:\\Teile\\")).unwrap();
    assert_eq!(p.changes.len(), 1, "doppelt: keine Änderung");
    p.remove_favorite(Path::new("D:\\Anders"));
    assert_eq!(p.changes.len(), 1);
    p.remove_favorite(Path::new("D:\\Teile"));
    assert!(p.changed() && p.favorites.is_empty());
    assert_eq!(
        p.changes[1],
        FavoriteChange::Remove(PathBuf::from("D:\\Teile"))
    );
}

#[test]
fn favorite_labels_tell_equal_names_apart() {
    let favs = [
        win("D:\\Kunde A\\Teile"),
        win("D:\\Kunde B\\Teile"),
        win("D:\\Muster"),
        win("E:\\"),
    ];
    let s = sep();
    assert_eq!(
        favorite_labels(&favs),
        [
            format!("Kunde A{s}Teile"),
            format!("Kunde B{s}Teile"),
            "Muster".to_string(),
            "E:".to_string(),
        ]
    );
}

/// Arbeitsbereich, der seine Konfiguration in `file` schreibt (nie in die echte).
fn workspace_at(file: &Path, config: Config) -> Workspace {
    let mut ws = Workspace::new(None, config);
    ws.config_path = file.to_path_buf();
    ws
}

#[test]
fn favorites_persist_through_the_workspace_and_the_config_file() {
    let d = temp_dir("speichern");
    let root = d.join("Projekte");
    let file = d.join("planora").join("config.json");
    let mut ws = workspace_at(
        &file,
        Config {
            root: Some(root.clone()),
            ..Config::default()
        },
    );
    let mut places = ws.places();
    assert_eq!(places.project_root.as_deref(), Some(root.as_path()));
    assert!(
        !ws.save_places(places.clone()),
        "ohne Änderung nichts speichern"
    );
    assert!(!file.exists(), "nichts geschrieben");
    places.add_favorite(&d.join("Teile")).unwrap();
    assert!(ws.save_places(places), "geänderte Favoriten speichern");
    assert_eq!(ws.config.favorites, [d.join("Teile")]);
    assert_eq!(ws.places().favorites, [d.join("Teile")]);
    // Über eine Sitzung hinaus.
    assert_eq!(Config::load_from(&file).favorites, [d.join("Teile")]);
    // Entfernen ebenso.
    let mut places = ws.places();
    places.remove_favorite(&d.join("Teile"));
    assert!(ws.save_places(places));
    assert!(Config::load_from(&file).favorites.is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_stale_instance_keeps_the_root_and_favorites_of_another() {
    let d = temp_dir("zwei");
    let file = d.join("config.json");
    // A startet ohne Datei (Wurzel None, keine Favoriten).
    let mut a = workspace_at(&file, Config::load_from(&file));
    // B startet, wählt eine Wurzel und einen Favoriten.
    let mut b = workspace_at(&file, Config::load_from(&file));
    b.config.root = Some(d.join("Y"));
    b.persist_config();
    let mut pb = b.places();
    pb.add_favorite(&d.join("Von B")).unwrap();
    assert!(b.save_places(pb));
    // A (veralteter Stand im Speicher) fügt einen Favoriten hinzu.
    let mut pa = a.places();
    pa.add_favorite(&d.join("Von A")).unwrap();
    assert!(a.save_places(pa));
    let disk = Config::load_from(&file);
    assert_eq!(disk.root, Some(d.join("Y")), "Wurzel von B bleibt");
    assert_eq!(disk.favorites, [d.join("Von B"), d.join("Von A")]);
    assert_eq!(a.config.favorites, disk.favorites, "A übernimmt den Stand");
    // A entfernt den Favoriten von B: nur dieser geht.
    let mut pa = a.places();
    pa.remove_favorite(&d.join("Von B"));
    assert!(a.save_places(pa));
    assert_eq!(Config::load_from(&file).favorites, [d.join("Von A")]);
    // B wählt eine neue Wurzel: die Favoriten von A bleiben.
    b.config.root = Some(d.join("Z"));
    b.persist_config();
    let disk = Config::load_from(&file);
    assert_eq!(disk.root, Some(d.join("Z")));
    assert_eq!(disk.favorites, [d.join("Von A")]);
    assert_eq!(b.config.favorites, disk.favorites, "B übernimmt den Stand");
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_config_unreadable_at_start_is_not_overwritten_later() {
    let d = temp_dir("kurz-unlesbar");
    let file = d.join("config.json");
    let good = r#"{ "root": "D:/Projekte", "favorites": ["D:/Alt"] }"#;
    // Beim Start kurz unlesbar: Wurzel None im Speicher.
    fs::write(&file, "{ halb").unwrap();
    let mut ws = workspace_at(&file, Config::load_from(&file));
    assert!(ws.config.root.is_none());
    // Solange unlesbar: nicht überschreiben, Favorit nur im Speicher.
    let mut p = ws.places();
    p.add_favorite(&d.join("Neu")).unwrap();
    assert!(!ws.save_places(p));
    assert_eq!(fs::read_to_string(&file).unwrap(), "{ halb");
    assert_eq!(ws.config.favorites, [d.join("Neu")]);
    assert_eq!(ws.favorites_pending.len(), 1, "vorgemerkt");
    // Ein Bild ohne Änderung schreibt nichts und vergisst nichts.
    let p = ws.places();
    assert!(!ws.save_places(p));
    assert_eq!(ws.config.favorites, [d.join("Neu")]);
    // Später wieder lesbar: die Wurzel der Datei bleibt, der vorgemerkte Favorit kommt mit.
    fs::write(&file, good).unwrap();
    let mut p = ws.places();
    p.add_favorite(&d.join("Noch einer")).unwrap();
    assert!(ws.save_places(p));
    let disk = Config::load_from(&file);
    assert_eq!(disk.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(
        disk.favorites,
        [PathBuf::from("D:/Alt"), d.join("Neu"), d.join("Noch einer")]
    );
    assert_eq!(ws.config.favorites, disk.favorites, "nichts verschwindet");
    assert!(ws.favorites_pending.is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn unsaved_removals_are_kept_too_and_the_root_choice_writes_them() {
    let d = temp_dir("vorgemerkt");
    let file = d.join("config.json");
    fs::write(
        &file,
        r#"{ "root": "D:/Projekte", "favorites": ["D:/A", "D:/B"] }"#,
    )
    .unwrap();
    let mut ws = workspace_at(&file, Config::load_from(&file));
    // Die Datei wird unlesbar (z. B. gesperrt); Entfernen und Hinzufügen bleiben vorgemerkt.
    fs::write(&file, "gesperrt").unwrap();
    let mut p = ws.places();
    p.remove_favorite(Path::new("D:/A"));
    p.add_favorite(&d.join("C")).unwrap();
    assert!(!ws.save_places(p));
    assert_eq!(ws.config.favorites, [PathBuf::from("D:/B"), d.join("C")]);
    // Wieder lesbar; die nächste Schreibgelegenheit ist eine Wurzelwahl.
    fs::write(
        &file,
        r#"{ "root": "D:/Projekte", "favorites": ["D:/A", "D:/B"] }"#,
    )
    .unwrap();
    ws.config.root = Some(PathBuf::from("E:/Neu"));
    ws.persist_config();
    let disk = Config::load_from(&file);
    assert_eq!(disk.root, Some(PathBuf::from("E:/Neu")));
    assert_eq!(disk.favorites, [PathBuf::from("D:/B"), d.join("C")]);
    assert_eq!(ws.config.favorites, disk.favorites);
    assert!(ws.favorites_pending.is_empty());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_full_list_on_disk_reports_instead_of_dropping_silently() {
    let d = temp_dir("platte-voll");
    let file = d.join("config.json");
    let mut a = workspace_at(&file, Config::load_from(&file));
    let mut b = workspace_at(&file, Config::load_from(&file));
    // B füllt die Liste auf der Platte.
    let mut pb = b.places();
    for i in 0..MAX_FAVORITES {
        pb.add_favorite(&d.join(format!("B{i}"))).unwrap();
    }
    assert!(b.save_places(pb));
    // A (leere Liste im Speicher) fügt einen hinzu.
    let mut pa = a.places();
    pa.add_favorite(&d.join("Von A")).unwrap();
    a.save_places(pa);
    let disk = Config::load_from(&file);
    assert_eq!(disk.favorites.len(), MAX_FAVORITES);
    assert!(!disk.favorites.contains(&d.join("Von A")));
    assert_eq!(
        a.config.favorites, disk.favorites,
        "A zeigt den Stand der Platte"
    );
    // Die nächste Dateiauswahl zeigt die Meldung.
    let mut shared = a.places();
    assert_eq!(shared.notice, Some(favorites_full()));
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    ctx.run_ui(egui::RawInput::default(), |ui| {
        c.show_with(ui, &mut shared);
    })
    .drop_without_applying_deltas();
    assert_eq!(c.error.as_deref(), Some(favorites_full().as_str()));
    assert_eq!(shared.notice, None, "übernommen");
    // Danach verschwindet sie aus dem Arbeitsbereich.
    a.save_places(shared);
    assert_eq!(a.places().notice, None);
    let _ = fs::remove_dir_all(&d);
}

// --- Zeilen der Liste --------------------------------------------------------------------------

fn dir_entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        is_dir: true,
    }
}

fn file_entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        is_dir: false,
    }
}

#[test]
fn folders_get_the_folder_symbol_files_the_file_symbol() {
    assert_eq!(EntryIcon::of(&dir_entry("A")), EntryIcon::Folder);
    assert_eq!(EntryIcon::of(&file_entry("a.stl")), EntryIcon::File);
    assert_eq!(EntryIcon::Folder.icon(), Icon::Folder);
    assert_eq!(EntryIcon::File.icon(), Icon::File);
    assert_eq!(icons::phosphor_name(Icon::Folder), "folder");
    assert_eq!(icons::phosphor_name(Icon::File), "file");
    for dark in [false, true] {
        let folder = EntryIcon::Folder.color(dark);
        assert_eq!(folder, theme::folder_icon_color(dark));
        assert_ne!(folder, EntryIcon::File.color(dark));
        let bg = theme::palette(dark).bg_panel;
        assert!(
            theme::contrast_ratio(folder, bg) >= 3.0,
            "Ordnersymbol lesbar (dunkel: {dark})"
        );
        assert!(theme::contrast_ratio(EntryIcon::File.color(dark), bg) >= 3.0);
    }
}

#[test]
fn the_open_hint_is_only_a_tooltip_of_folders() {
    let a = dir_entry("A");
    assert_eq!(entry_text(&a), "A");
    assert_eq!(entry_hover(&a), Some("Doppelklick zum Öffnen".to_string()));
    assert_eq!(entry_hover(&file_entry("a.stl")), None);
    assert_eq!(entry_text(&file_entry("a.stl")), "a.stl");
}

#[test]
fn only_folder_rows_and_favorite_buttons_have_a_context_menu() {
    let dir = &win("D:\\Ablage");
    assert_eq!(row_menu(&file_entry("a.stl"), dir, &[]), None);
    let add = row_menu(&dir_entry("Teile"), dir, &[]).unwrap();
    assert_eq!(add, RowMenu::AddFavorite(dir.join("Teile")));
    assert_eq!(add.label(), "Zu Favoriten hinzufügen");
    let favs = [win("D:\\Ablage\\Teile\\")];
    let remove = row_menu(&dir_entry("Teile"), dir, &favs).unwrap();
    assert_eq!(remove, RowMenu::RemoveFavorite(dir.join("Teile")));
    assert_eq!(remove.label(), "Aus Favoriten entfernen");
    let fav = Place {
        kind: PlaceKind::Favorite,
        label: "Teile".into(),
        path: dir.join("Teile"),
    };
    assert_eq!(
        place_menu(&fav),
        Some(RowMenu::RemoveFavorite(dir.join("Teile")))
    );
    let drive = Place {
        kind: PlaceKind::Drive,
        label: "C:".into(),
        path: PathBuf::from("C:\\"),
    };
    assert_eq!(place_menu(&drive), None);
}

#[test]
fn the_row_menu_adds_and_removes_favorites() {
    let d = temp_dir("menue");
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let mut shared = Places::default();
    c.apply_menu(&RowMenu::AddFavorite(d.join("Teile")), &mut shared);
    assert_eq!(shared.favorites, [d.join("Teile")]);
    assert!(shared.changed());
    c.apply_menu(&RowMenu::RemoveFavorite(d.join("Teile")), &mut shared);
    assert!(shared.favorites.is_empty());
    let _ = fs::remove_dir_all(&d);
}

// --- Klick auf einen Schnellzugriff ------------------------------------------------------------

#[test]
fn a_place_changes_the_folder_or_reports_a_missing_one() {
    let d = temp_dir("ziel");
    fs::create_dir_all(d.join("Teile")).unwrap();
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let to = |kind, path: PathBuf| Place {
        kind,
        label: "x".into(),
        path,
    };
    assert!(c.go_to_place(&to(PlaceKind::Desktop, d.join("Teile"))));
    assert_eq!(c.dir(), d.join("Teile").as_path());
    assert!(c.error.is_none());
    assert!(!c.go_to_place(&to(PlaceKind::Drive, d.join("fehlt"))));
    assert_eq!(c.dir(), d.join("Teile").as_path(), "bleibt im Ordner");
    assert_eq!(c.error.as_deref(), Some("Diesen Ordner gibt es nicht."));
    assert!(!c.go_to_place(&to(PlaceKind::Favorite, d.join("fehlt"))));
    assert_eq!(c.error.as_deref(), Some(favorite_missing().as_str()));
    let _ = fs::remove_dir_all(&d);
}

// --- Gezeichnet --------------------------------------------------------------------------------

struct Driver {
    ctx: egui::Context,
    time: f64,
}

impl Driver {
    fn new() -> Driver {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        Driver { ctx, time: 0.0 }
    }

    /// Ein Bild; liefert alle Texte mit ihrer Mitte.
    fn frame(
        &mut self,
        c: &mut FileChooser,
        shared: &mut Places,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Pos2)> {
        self.time += 0.05;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(input, |ui| {
            c.show_with(ui, shared);
        });
        fn walk(s: &egui::epaint::Shape, out: &mut Vec<(String, egui::Pos2)>) {
            match s {
                egui::epaint::Shape::Text(t) => {
                    out.push((t.galley.text().to_string(), t.pos + t.galley.size() / 2.0))
                }
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut seen = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn click(
        &mut self,
        c: &mut FileChooser,
        shared: &mut Places,
        at: egui::Pos2,
        button: egui::PointerButton,
    ) {
        let ev = |pressed| egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(c, shared, vec![egui::Event::PointerMoved(at)]);
        self.frame(c, shared, vec![ev(true)]);
        self.frame(c, shared, vec![ev(false)]);
        self.frame(c, shared, vec![]);
    }
}

fn pos_of(seen: &[(String, egui::Pos2)], text: &str) -> Option<egui::Pos2> {
    seen.iter().find(|(t, _)| t == text).map(|(_, p)| *p)
}

fn has(seen: &[(String, egui::Pos2)], part: &str) -> bool {
    seen.iter().any(|(t, _)| t.contains(part))
}

#[test]
fn rows_show_symbol_and_name_without_the_bracket_text() {
    let d = temp_dir("zeilen");
    fs::create_dir_all(d.join("Ordner A")).unwrap();
    fs::write(d.join("teil.stl"), b"x").unwrap();
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let mut shared = Places::default();
    let mut drv = Driver::new();
    drv.frame(&mut c, &mut shared, vec![]);
    let seen = drv.frame(&mut c, &mut shared, vec![]);
    // Name als eigener Text, davor das Symbol (Ordner bzw. Datei) auf derselben Zeile.
    for (name, icon, other) in [
        ("Ordner A", Icon::Folder, Icon::File),
        ("teil.stl", Icon::File, Icon::Folder),
    ] {
        let at = pos_of(&seen, name).unwrap_or_else(|| panic!("{name}: {seen:?}"));
        let symbol = |i: Icon| {
            seen.iter().any(|(t, p)| {
                *t == icons::glyph(i).to_string() && (p.y - at.y).abs() < 4.0 && p.x < at.x
            })
        };
        assert!(symbol(icon), "{name}: Symbol {icon:?}");
        assert!(!symbol(other), "{name}: nicht {other:?}");
    }
    assert!(!has(&seen, "Doppelklick"), "Hinweis nur im Tooltip");
    assert!(!has(&seen, "(Ordner"));
    // Die ganze Auswahl zeichnet auch ihre Knöpfe.
    assert!(pos_of(&seen, "Öffnen").is_some() && pos_of(&seen, "Abbrechen").is_some());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn clicking_a_place_button_enters_its_folder() {
    let d = temp_dir("knopf");
    let root = d.join("Projekte");
    fs::create_dir_all(&root).unwrap();
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let mut shared = Places {
        project_root: Some(root.clone()),
        ..Places::default()
    };
    let mut drv = Driver::new();
    drv.frame(&mut c, &mut shared, vec![]);
    let seen = drv.frame(&mut c, &mut shared, vec![]);
    let at = pos_of(&seen, &project_label()).expect("Knopf „Projekte“");
    drv.click(&mut c, &mut shared, at, egui::PointerButton::Primary);
    assert_eq!(c.dir(), root.as_path());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn right_click_on_a_folder_offers_favorites_but_not_on_a_file() {
    let d = temp_dir("rechts");
    fs::create_dir_all(d.join("Teile")).unwrap();
    fs::write(d.join("teil.stl"), b"x").unwrap();
    let mut c = FileChooser::open(d.clone(), vec!["stl"]);
    let mut shared = Places::default();
    let mut drv = Driver::new();
    drv.frame(&mut c, &mut shared, vec![]);
    let seen = drv.frame(&mut c, &mut shared, vec![]);

    // Datei: kein Menü.
    let at = pos_of(&seen, "teil.stl").expect("Datei");
    drv.click(&mut c, &mut shared, at, egui::PointerButton::Secondary);
    let seen = drv.frame(&mut c, &mut shared, vec![]);
    assert!(!has(&seen, &add_favorite_label()), "kein Menü an Dateien");
    assert_eq!(c.file_name, "", "Rechtsklick wählt keine Datei");
    // Menü schließen (Escape gibt es nicht im Treiber: Klick ins Leere).
    drv.click(
        &mut c,
        &mut shared,
        egui::pos2(790.0, 590.0),
        egui::PointerButton::Primary,
    );

    // Ordner: Menü mit „Zu Favoriten hinzufügen“, Klick fügt ihn hinzu.
    let seen = drv.frame(&mut c, &mut shared, vec![]);
    let at = pos_of(&seen, "Teile").expect("Ordner");
    drv.click(&mut c, &mut shared, at, egui::PointerButton::Secondary);
    let seen = drv.frame(&mut c, &mut shared, vec![]);
    let item = pos_of(&seen, &add_favorite_label()).expect("Menüeintrag");
    drv.click(&mut c, &mut shared, item, egui::PointerButton::Primary);
    assert_eq!(shared.favorites, [d.join("Teile")]);
    assert!(shared.changed());
    assert_eq!(c.dir(), d.as_path(), "Rechtsklick öffnet den Ordner nicht");

    // Der neue Favorit erscheint als Knopf.
    let seen = drv.frame(&mut c, &mut shared, vec![]);
    assert_eq!(
        seen.iter().filter(|(t, _)| t == "Teile").count(),
        2,
        "Zeile und Favoriten-Knopf: {seen:?}"
    );
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn both_import_dialogs_show_the_shared_places() {
    use planora_app::import_dialog::ImportDialog;
    use planora_app::state::AppState;
    use planora_app::step_import_dialog::StepImportDialog;
    let d = temp_dir("dialoge");
    let root = d.join("Projekte");
    fs::create_dir_all(&root).unwrap();
    let mut shared = Places {
        project_root: Some(root),
        favorites: vec![d.join("Lieblingsordner")],
        ..Places::default()
    };
    let texts = |f: &mut dyn FnMut(&egui::Context)| {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        let mut out = String::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let o = ctx.run_ui(input, |ui| f(ui.ctx()));
            out = planora_app::testing::collect_texts(&o);
            o.drop_without_applying_deltas();
        }
        out
    };
    let mut s = AppState::new();
    let mut last = None;
    let mut mesh = ImportDialog::new(d.clone());
    let seen = texts(&mut |ctx| {
        mesh.show_in(ctx, &mut s, &mut last, &mut shared);
    });
    assert!(seen.contains(&project_label()), "{seen}");
    assert!(seen.contains("Lieblingsordner"), "{seen}");
    let mut step = StepImportDialog::new(d.clone());
    let seen = texts(&mut |ctx| {
        step.show_in(ctx, &mut s, &mut last, &mut shared);
    });
    assert!(seen.contains(&project_label()), "{seen}");
    assert!(seen.contains("Lieblingsordner"), "{seen}");
    let _ = fs::remove_dir_all(&d);
}

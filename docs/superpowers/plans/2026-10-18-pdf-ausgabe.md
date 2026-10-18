# PDF-Ausgabe der Zeichnungen (Teilprojekt 3c-3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **COMMIT-TRAILER (jeder Commit, ohne Ausnahme, wörtlich als letzte Zeile):**
> `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`
> Frühere Agents haben hier wiederholt „Opus“ oder einen anderen Namen eingesetzt. Vor jedem `git commit` die Zeile Zeichen für Zeichen prüfen.
> Immer nur die genannten Pfade einzeln `git add`en (nie `git add -A`/`.`/`-u`): der unverfolgte Ordner `assets/` darf nie in einen Commit.

**Goal:** Das Export-Fenster schreibt Zeichnungen als PDF – das gewählte Blatt oder alle Blätter des Dokuments in einer mehrseitigen Datei, mit oder ohne Blattrahmen –, selbst geschrieben ohne neue Bibliothek, inhaltlich gleich der SVG-Ausgabe und Byte für Byte reproduzierbar.

**Architecture:** `planora-drawing` bekommt zwei reine Bausteine: `pdf_font.rs` (WinAnsi-Zeichensatz, `pdf_char_ok`, Helvetica-Breiten aus den Adobe-Core-14-Metriken) und `pdf.rs` (Schreiber: Objekte, Querverweistabelle, Seitenbaum, ein unkomprimierter Inhaltsstrom je Seite; jedes SVG-Element wird genau ein PDF-Element). `file.rs` bekommt `DrawingFormat::Pdf` und `export_pdf_file` über dieselbe Hilfsdatei-Logik. Die App erweitert das Export-Fenster um das Format „Zeichnung als PDF“, die Wahl „Gewähltes Blatt“/„Alle Blätter“, die Zeichenprüfung mit Feldnamen und die Berechnung aller Blätter im Hintergrund (Sperre nach F2). Ein eigener PDF-Leser in den Tests prüft Aufbau und Inhalt; Golden-Dateien sichern die Bytes.

**Tech Stack:** Rust (Workspace, keine neuen Abhängigkeiten), egui/eframe 0.36; Tests im eigenen Harness (`egui::Context::run_ui`, `AppState`, `ExportDialog`), SVG/DXF-Leser in `crates/planora-drawing/tests/common/`, neu `common/pdf_read.rs`.

**Spec:** `docs/superpowers/specs/2026-10-18-pdf-ausgabe-design.md` (bindend, mit §8 Festlegungen F1/F2). Vorbilder für Stil, Muster und Lehren: `docs/superpowers/plans/2026-10-17-zeichnungsblatt.md` (3c-1) und `docs/superpowers/plans/2026-10-18-stueckliste.md` (3c-2, mit „Abweichungen bei der Umsetzung“). Basis: `master` @ `4840dfe`; Arbeitszweig `feature/pdf-ausgabe` in einem eigenen Worktree (das Hauptverzeichnis steht auf `feature/logo` mit unfertiger Arbeit des Nutzers – dort nichts anfassen). Spec und Plan sind beim Planen unverfolgt; der Controller committet beide als Erstes auf dem Arbeitszweig.

## Global Constraints

- Texte deutsch in einfacher Laiensprache, keine technischen Hinweise, keine Hinweise auf Datenherkunft (Nutzerregel). Feste Texte: Format „Zeichnung als PDF“, Wahl „Gewähltes Blatt“ / „Alle Blätter“ (Vorgabe gewähltes Blatt), Zeichenmeldung „Das Zeichen „X“ im Feld „Y“ lässt sich in einer PDF nicht ausgeben.“ (Spec §4), Fortschritt „Blatt n von N“ (F2).
- **Keine neuen Abhängigkeiten**: die PDF wird von Hand geschrieben; `Cargo.toml`/`Cargo.lock` ohne neue Pakete; `cargo deny check` grün mit den drei in `deny.toml` vermerkten zbus-Ausnahmen; `deny.toml` bleibt unberührt.
- Lizenzen (Nutzerregel): Die Helvetica-Breitentabelle stammt aus den Adobe-Core-14-AFM-Dateien (`Helvetica.afm`, AFM 4.1, Version 002.000); Herkunft, Copyright-Zeile und Nutzungsbedingungen der AFM-Sammlung stehen im Kopf von `pdf_font.rs` und im README-Abschnitt „Drittanbieter“. Lässt sich der Wortlaut der Bedingungen nicht nachprüfen, stoppt der Implementierer und meldet es dem Controller (nicht raten). Kein Autodesk-Material.
- PDF 1.4, klassische Querverweistabelle, unkomprimierte Inhaltsströme, die ganze Datei nur ASCII; Seite = Blattgröße in pt (1 mm = 72/25,4 pt), Nullpunkt unten links; Zahlen auf 4 Nachkommastellen (pt); nicht endliche Zahlen → nichts geschrieben (Spec §3, §5).
- Inhalt je Seite = genau der Inhalt der SVG-Ausgabe für dieses Blatt (dieselbe Quelle `SheetModel`; kein zweiter Berechnungsweg); Linienstärken, Strichmuster, Farben wie SVG.
- **F1:** Kein eigenes Einpassen oder Kürzen im PDF-Schreiber; die Helvetica-Breiten dienen nur dem Zentrieren/Rechtsbündigsetzen (und der Stauchung P6). **F2:** „Alle Blätter“ wird ganz gesperrt, solange ein Blatt nicht ausgegeben werden kann; Meldung nennt Blatt und Grund (erster Fehler in Blattreihenfolge); nie Blätter weglassen, nie Teil-PDF.
- Grenzen: höchstens 200 Seiten, höchstens 14 400 pt je Seitenkante (Spec §5).
- Byte-gleiche Ausgabe bei gleicher Eingabe (keine Uhrzeit, keine Zufallskennung, keine `/ID` im Schreiber; das heutige Datum kommt nur als Parameter von der App).
- Farben nur über `theme::` (Wächter `no_hard_coded_colors`), Fenster nur über `dialog::Dialog`, Zeichen nur aus Montserrat/Symbolschrift (`tests/fonts.rs`); gleiche Kartenhöhe in einer Reihe (bestehender Test `cards_stay_centred_720_with_equal_heights`).
- Geschützte Pfade unverändert gegenüber `4840dfe`: `crates/planora-kernel/**`, `crates/planora-mesh/src/**`, `crates/planora-step/**`, `crates/planora-exchange/**`.
- Nach jeder Aufgabe: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, betroffene Tests mit `-j 2` **im Vordergrund** (langsamer Rechner; nie `cargo clean`; keine Hintergrundprozesse zurücklassen). Am Ende `cargo test --workspace -j 2` und `cargo deny check`.
- **Tests müssen beißen:** Jede Aufgabe endet mit mindestens **10 Mutationen**, die der Implementierer einzeln einspielt, den roten Test notiert und wieder zurücknimmt; der Bericht listet „Mutation → roter Test“. Eine Mutation, die grün bleibt, verlangt einen schärferen Test, nicht eine andere Mutation. (Lehre aus 3c-1/3c-2: Tests haben mehrfach nicht gebissen.)
- Keine Streudateien: Testdateien nur in `std::env::temp_dir()`-Unterordnern, am Testende entfernt; Proben nur über den ausgeschalteten Test nach `D:\Testumgebung\zeichnung-proben\pdf\`; `git status` zeigt am Ende nur `assets/` unverfolgt.
- Commits: nur genannte Pfade einzeln `git add`; Trailer siehe Kasten oben.
- Abweichungen von der Spec stehen im Ledger des Controllers und in `docs/superpowers/offene-punkte.md`; Werte der Spec ändern sich nur nach Zustimmung des Nutzers.

**Festlegungen des Plans (P1–P14; der Nutzer kann sie kippen):**
- **P1 Dateiaufbau** (Zeilenende `\n`, ein Leerzeichen zwischen Elementen): `%PDF-1.4`; Objekt 1 `<< /Type /Catalog /Pages 2 0 R >>`; 2 `<< /Type /Pages /Kids [5 0 R 7 0 R …] /Count n >>`; 3 `<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>`; 4 Info (P7); je Seite i (ab 0) Objekt `5 + 2i` `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 w h] /Resources << /Font << /F1 3 0 R >> >> /Contents {6 + 2i} 0 R >>` und `6 + 2i` `<< /Length L >>\nstream\n…\nendstream`; dann `xref\n0 N\n` mit Einträgen zu genau 20 Bytes (`0000000000 65535 f \n`, `{offset:010} 00000 n \n`), `trailer\n<< /Size N /Root 1 0 R /Info 4 0 R >>\nstartxref\n{offset}\n%%EOF\n`. `L` = Bytes zwischen `stream\n` und `\nendstream`.
- **P2 Zahlen:** pt = mm · 72/25,4 (`PT_PER_MM`), auf 4 Nachkommastellen, ohne Exponent, ohne nachgestellte Nullen, `-0` → `0` (wie `svg::num`, nur 4 statt 6 Stellen); gilt auch für `MediaBox`, `w`, `d`, `Tf`, `Tm`, `Tz`. Nicht endlich → `DrawError::NonFinite`, nichts geschrieben.
- **P3 Inhaltsstrom:** beginnt mit `1 J 1 j` (runde Enden/Ecken wie SVG). Reihenfolge = SVG-Reihenfolge (Rahmen, dann je Ansicht sichtbarer Pfad, verdeckter Pfad, Gruppen Maße/Mittellinien/Positionen wie `svg::mark_groups`); **jedes SVG-Element wird genau ein PDF-Element**: Strich `{Farbe} RG {w} w [{Muster}] 0 d` + Pfad + `S`; Fläche (Pfeil) `{Farbe} rg` + `m l l h f`; Text `{Farbe} rg BT /F1 {Größe} Tf {a b c d e f} Tm [{tz} Tz] ({…}) Tj [100 Tz] ET`. Schwarz `0 0 0`, verloren (`MarkKind::Lost`) `0.8 0 0` (= SVG `#c00`). Jede Anweisung eine Zeile.
- **P4 Strichbreiten** wie `svg.rs`: sichtbar 0,5, verdeckt 0,25 mit Muster 3 1 (mm); `Dimension`/`Lost` `DIM_LINE_W`; `CenterLine` `CENTER_W` mit `CENTER_PATTERN`; Rahmenmarken `pen.width_at(frame.scale)`; Ansichtsmarken `Sheet`/`Position` mit `width_at(sheet_scale(model.sheet))`; alles in pt umgerechnet.
- **P5 Kurven:** Kreis- und Ellipsenbögen über `Out2d::normalized_range`, `n = ceil(span/(π/2) − 1e-9)` Stücke (wie SVG), je Stück kubisch `P0 = P(t0)`, `P1 = P0 + κ·P′(t0)`, `P2 = P3 − κ·P′(t1)`, `P3 = P(t1)`, `κ = 4/3 · tan(h/4)` (h = Stückspanne), Punkte aus derselben Parameterform wie SVG (negativer Radius, Ellipsen-`ratio` < 0 wirken von selbst); voller Umlauf endet exakt im Anfangspunkt. `Mark::Arc` ebenso (r ≤ 0 oder Spanne ≤ 0 → nichts). Splines: Grad 1 → `l`, Grad 2 nicht rational → Gradanhebung (`P0, P0 + ⅔(P1 − P0), P2 + ⅔(P1 − P2), P2`), Grad 3 nicht rational → `c`, sonst die kubische Näherung aus `svg.rs` (gemeinsam genutzt, Algorithmus und SVG-Bytes unverändert). Neuer Teilpfad (`m`) nur wie in `svg::spline_path`.
- **P6 Texte:** `/F1` Helvetica, WinAnsi; Größe = Texthöhe (mm → pt); Anker: Ursprung = Ankerpunkt − k·W·(cos α, sin α), k = 0 / ½ / 1 für Start / Mitte / Ende, W = Helvetica-Breite (nach Stauchung); Drehung über `Tm [cos α, sin α, −sin α, cos α, x, y]`. **Stauchung:** Ist die Helvetica-Breite größer als die Schätzung, mit der das Blattmodell gerechnet hat (`sheet_text_width` für `Sheet`/`Position`, `annotate::text_width` sonst), wird mit `Tz = 100 · Schätzung / Helvetica-Breite` gestaucht und danach `100 Tz` gesetzt – so ragt nie ein Text über das hinaus, was SVG-Bild und Einpassen angenommen haben (ändert keinen Textinhalt; siehe Frage Q1). `−` (U+2212) wird `-`; `( ) \` werden maskiert, Bytes ≥ 0x80 als `\ddd` (oktal, 3 Stellen). Ein Zeichen ohne WinAnsi-Platz → `DrawError::Unsupported(pdf_char_message(c))`, nichts geschrieben, nichts ersetzt.
- **P7 Dokumenteigenschaften (Info):** `/Producer (PlanoraCAD) /Creator (PlanoraCAD)`, `/Title` und `/Author` als UTF-16BE-Hex mit BOM (`<FEFF…>`, jedes Zeichen möglich, auch außerhalb WinAnsi), `/Title` fehlt bei leerem Titel, `/Author` fehlt bei `None`/leer; `/CreationDate (D:JJJJMMTT000000Z)` nur bei gültigem Datum `JJJJ-MM-TT` (Monat 1–12, Tag 1–31), sonst fehlt es. Keine `/ID`, kein `/ModDate`.
- **P8 Grenzen im Schreiber:** 0 Seiten → `Unsupported(NO_PAGES)`; mehr als `MAX_PAGES = 200` → `Unsupported(TOO_MANY_PAGES)`; eine Kante > `MAX_PAGE_PT = 14 400` → `Unsupported(PAGE_TOO_LARGE)` (heute mit A4…A0 unerreichbar, trotzdem geprüft).
- **P9 Auswahl im Fenster:** Nur bei PDF erscheint in der Karte „Zeichnung“ die Wahl `PDF_SELECTED`/`PDF_ALL` (Vorgabe `PdfPages::Selected`). Bei „Alle Blätter“ ist die Zeichnungsauswahl gesperrt (Hinweis `PDF_ALL_HINT`), die Angaben zeigen „Blätter: {n}“ und die Summen. „Blattrahmen mit ausgeben“ gilt für jedes Blatt.
- **P10 Dateiname und Titel:** gewähltes Blatt → Zeichnungsname; alle Blätter → Dokumentname (`state.document_name`); unzulässige Zeichen → `_` wie heute in `begin_save`; `.pdf` erzwingt `normalize_target` (wie SVG/DXF).
- **P11 Metadaten aus der App** (`pdf_meta`): Titel nach P10; Ersteller = eigener Text von `project_info().creator` (`FieldValue::Text`, nicht leer), sonst keiner (nicht der Anmeldename; siehe Frage Q2); Datum = erstes Blatt der PDF: eigenes Ausgabedatum, wenn gültig, sonst `info.created`, wenn gültig, sonst `today` (von der App: `Stamp::now().date`, UTC).
- **P12 Alle Blätter:** ein Hintergrundauftrag rechnet die Blätter in Dokumentreihenfolge nacheinander mit `prepare_drawing_parts` (je Blatt eigener `FrameInput` mit `context_or_single`, eigener `PartsInput`; beides ohne DXF-Zuschlag) und dem Fortschritt des Auftrags (der Balken zeigt das laufende Blatt); ein Zähler (`Arc<AtomicUsize>`) liefert den Text `sheet_progress_text(n, N)`. Der erste Fehler in Blattreihenfolge → `Err(in_sheet(Blattname, Meldung))`, nichts geschrieben. Gesperrt vor dem Rechnen (in `drawing_block`): mehr als 200 Zeichnungen (`too_many_pages_text`), eine Ansichtenzeichnung ohne Ansicht (vorhandene Meldung, nennt das Blatt schon). Netz-Körper-Hinweis einmal.
- **P13 Zeichenprüfung in der App** (vor dem Schreiben, mit Namen): `pdf_sheet_texts_ok(texts)` mit denselben Feldnamen wie `dxf_sheet_texts_ok` (gemeinsamer Helfer; die DXF-Meldungen bleiben wörtlich gleich) und `pdf_model_texts_ok(model)` für die Texte der Ansichten („Das Zeichen „X“ in einem Maßtext der Ansicht „V“ lässt sich in einer PDF nicht ausgeben.“). `−`, `…`, `–`, `Ø`, `°`, `±`, `µ`, `€` sind zulässig.
- **P14 Hilfsdatei:** `export_drawing_file` und `export_pdf_file` teilen `write_via_temp(path, bytes, progress)` in `file.rs` (Hilfsdatei `.<Name>.planora-tmp`, `sync_all`, umbenennen, bei Fehler/Abbruch Hilfsdatei weg, Ziel unverändert); Verhalten von SVG/DXF bleibt Byte für Byte.

## Review Focus

1. **Programmeigene Sonderzeichen sperren nicht:** Toleranz „−0,1“ (U+2212), gekürzter Text mit „…“, Vorgabe „–“, „Ø 12“, „± 0,1“, „20°“ → PDF wird geschrieben, `−` steht als `-` in der Datei. → Aufgabe 1 Test (b), Aufgabe 2 Test (i), Aufgabe 4 Test (d).
2. **Kein Text ragt aus seiner Zelle:** breite Zeichen (`@`, `—`, `…`, `&`, Folgen von `O`) in einem bis an die Grenze gefüllten Feld → gerenderte Breite nie größer als die Schätzung des Blattmodells. → Aufgabe 2 Test (h).
3. **Ein kaputtes Blatt mitten in „Alle Blätter“:** verlorene Positionsnummer auf Blatt 3 von 5, Emoji auf Stücklisten-Folgeblatt 2 → keine Datei, keine Hilfsdatei, Meldung nennt das erste kaputte Blatt; nach dem Beheben gelingt die PDF mit allen Seiten. → Aufgabe 5 Tests (c), (d).
4. **Änderungen während der Berechnung aller Blätter:** Zeichnung hinzufügen/löschen/umbenennen, Dokument umbenennen, Rahmen-Schalter, Wechsel gewählt ↔ alle → altes Ergebnis wird nie geschrieben (`STALE`), die neue PDF hat die neue Seitenzahl und Reihenfolge. → Aufgabe 5 Test (e).
5. **Betrachter verschlucken sich nicht:** exakte Versätze und Längen, nur ASCII, Vollkreise, Bögen über 0°, winzige Bögen (Spanne 1e-9), negativer Radius, rationale und Grad-5-Splines, NaN → gültige Datei mit derselben Form wie SVG oder gar keine. → Aufgabe 2 Tests (a), (j), (k); Aufgabe 3 Test (a).

---

### Task 1: Zeichensatz und Helvetica-Breiten (`pdf_font.rs`)

**Files:**
- Create: `crates/planora-drawing/src/pdf_font.rs` (Kopf: Quelle `Helvetica.afm` aus Adobe Core 14 AFM, AFM 4.1, Version 002.000, Copyright-Zeile von Adobe, Nutzungsbedingungen der AFM-Sammlung wörtlich bzw. sinngemäß mit Fundstelle; Tabelle der WinAnsi-Belegung 0x80–0x9F nach PDF-Referenz 1.4, Anhang D)
- Modify: `crates/planora-drawing/src/lib.rs` (`pub mod pdf_font;`)
- Test: `crates/planora-drawing/tests/stage25_pdf_font.rs` (neu)

**Interfaces:**
- Consumes: nichts.
- Produces:
```rust
pub fn winansi_byte(c: char) -> Option<u8>;   // ASCII 0x20..=0x7E wie es ist; U+2212 → 0x2D; U+00A0..=U+00FF → gleiches Byte;
                                              // 27 Zeichen auf 0x80..=0x9F (€ ‚ ƒ „ … † ‡ ˆ ‰ Š ‹ Œ Ž ‘ ’ “ ” • – — ˜ ™ š › œ ž Ÿ); sonst None
pub fn pdf_char_ok(c: char) -> bool;          // winansi_byte(c).is_some()
pub fn helvetica_width(byte: u8) -> u16;      // 1/1000 Geviert; 0 für Bytes ohne Zeichen (0x00–0x1F, 0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D)
pub fn pdf_text_width(text: &str, size: f64) -> Option<f64>; // Summe · size / 1000; None bei einem unzulässigen Zeichen
```

- [ ] **Step 1: Failing tests (`stage25_pdf_font.rs`):** (a) `known_helvetica_widths` (über `helvetica_width(winansi_byte(c).unwrap())`: Leerzeichen 278, `'` (0x27, quotesingle) 191, `` ` `` (0x60, grave) 333, `"` 355, `%` 889, `&` 667, `-` 333, `.` 278, `0`–`9` je 556, `@` 1015, `A` 667, `M` 833, `O` 778, `W` 944, `i` 222, `l` 222, `m` 833, `w` 722, `Ä` 667, `Ö` 778, `Ü` 722, `ä` `ö` `ü` je 556, `ß` 611, `Ø` 778, `°` 400, `±` 584, `µ` 556, `€` 556, `…` 1000, `–` 556, `—` 1000, `Œ` 1000, NBSP 278, weiches Trennzeichen U+00AD 333); (b) `winansi_mapping` (`ä`→0xE4, `€`→0x80, `…`→0x85, `–`→0x96, `—`→0x97, `‚`→0x82, `Ÿ`→0x9F, `Œ`→0x8C, `−`→0x2D, U+00A0→0xA0; jedes ASCII 0x20..=0x7E auf sich selbst; `None` für `'\n'`, `'\t'`, U+007F, U+0080–U+009F als Zeichen, `'Ā'` U+0100, `'ő'` U+0151, `'😀'`, U+2010; Zahl der zulässigen Zeichen in U+0000..=U+FFFF genau 219 = 95 + 96 + 27 + 1); (c) `text_width` (`pdf_text_width("Helvetica", 10.0) == Some(41.12)` ±1e-9; `("Ä Ø ß", 3.5)` = (667 + 278 + 778 + 278 + 611)·0,0035 ±1e-12; `("−5", 1.0) == Some(0.889)`; `("a😀", 1.0) == None`; leer → `Some(0.0)`); (d) `every_allowed_char_has_a_width` (für jedes zulässige Zeichen Breite > 0; für die 5 freien Bytes 0x81, 0x8D, 0x8F, 0x90, 0x9D gibt es kein Zeichen mit diesem Byte).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage25_pdf_font -j 2`
- [ ] **Step 3: Implement** nach Interfaces: Breiten als `const` Tabelle über die Bytes 0x20..=0xFF (224 Einträge) aus `Helvetica.afm` nach WinAnsi-Glyphnamen (Achtung: AFM-Kodierung ist StandardEncoding; 0x27 = `quotesingle`, 0x60 = `grave`, nicht `quoteright`/`quoteleft`). Kopfkommentar mit Quelle und Lizenz (Global Constraints).
- [ ] **Step 4: Run → PASS**, `cargo test -p planora-drawing --lib -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10, Liste im Bericht)**, z. B.: 0x27 → 222; 0x60 → 222; `ß` 556; `@` 950; `−` nicht abgebildet; U+007F zulässig; Steuerzeichen zulässig; 0x80 (`€`) fehlt; 0x9F (`Ÿ`) fehlt; U+00AD fehlt; `pdf_text_width` ohne `/1000`; unzulässiges Zeichen als Breite 0 statt `None`.
- [ ] **Step 6: Commit** — `feat: WinAnsi-Zeichensatz und Helvetica-Breiten für die PDF-Ausgabe` (Pfade: `pdf_font.rs`, `lib.rs`, `stage25_pdf_font.rs`).

### Task 2: PDF-Schreiber (`pdf.rs`) und PDF-Leser im Test

**Files:**
- Create: `crates/planora-drawing/src/pdf.rs` (Modulkommentar mit P1–P8 wie in `svg.rs`/`dxf.rs`)
- Create: `crates/planora-drawing/tests/common/pdf_read.rs` (nur `std`, keine `super::`-Bezüge, damit die App-Tests ihn über `#[path]` einbinden können)
- Modify: `crates/planora-drawing/src/svg.rs` (nur: die kubische Näherung `adaptive_fit` samt Hilfen als `pub(crate) fn fitted_cubics(q: &[[f64; 3]], first: DVec2, last: DVec2) -> Vec<[DVec2; 3]>` freigeben – gleicher Algorithmus, SVG-Bytes unverändert), `crates/planora-drawing/src/lib.rs` (`pub mod pdf;`, `pub use pdf::{PdfMeta, pdf_bytes, write_pdf};`), `crates/planora-drawing/tests/common/mod.rs` (`pub mod pdf_read;`)
- Test: `crates/planora-drawing/tests/stage26_pdf.rs` (neu)

**Interfaces:**
- Consumes: `pdf_font::{winansi_byte, pdf_char_ok, pdf_text_width}` (1); `SheetModel`, `Mark`, `MarkKind`, `TextAnchor`, `Out2d::normalized_range`, `bezier_pieces`, `sheet_text_width`, `annotate::{text_width, ARROW_LEN, ARROW_HALF_W, DIM_LINE_W, CENTER_W, CENTER_PATTERN}`, `sheet_scale`.
- Produces:
```rust
pub const PT_PER_MM: f64 = 72.0 / 25.4;
pub const MAX_PAGES: usize = 200;
pub const MAX_PAGE_PT: f64 = 14_400.0;
pub const PRODUCER: &str = "PlanoraCAD";
pub const NO_PAGES: &str = "Es gibt kein Blatt für die PDF.";
pub const TOO_MANY_PAGES: &str = "Eine PDF kann höchstens 200 Blätter enthalten.";
pub const PAGE_TOO_LARGE: &str = "Das Blatt ist für eine PDF zu groß.";
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfMeta { pub title: String, pub author: Option<String>, pub date: Option<String> /* „JJJJ-MM-TT“ */ }
pub fn pdf_bytes(pages: &[&SheetModel], meta: &PdfMeta) -> Result<Vec<u8>, DrawError>;
pub fn write_pdf(pages: &[&SheetModel], meta: &PdfMeta, out: &mut dyn Write) -> Result<(), DrawError>; // bei Fehler nichts geschrieben
pub fn pdf_char_message(c: char) -> String;   // „Das Zeichen „{c}“ lässt sich in einer PDF nicht ausgeben.“
pub fn pdf_num(x: f64) -> Result<String, DrawError>;   // P2
pub fn pdf_date(date: &str) -> Option<String>;         // „2026-10-18“ → „D:20261018000000Z“, ungültig → None
pub fn fits_page_pt(w: f64, h: f64) -> bool;           // beide ≤ MAX_PAGE_PT
// tests/common/pdf_read.rs
pub struct Pdf { pub pages: Vec<Page>, pub info: BTreeMap<String, Value>, pub objects: BTreeMap<u32, Value> }
pub struct Page { pub media_box: [f64; 4], pub ops: Vec<Op>, pub items: Vec<Item> }
pub struct Op { pub op: String, pub args: Vec<Value> }
pub enum Value { Num(f64), Name(String), Str(Vec<u8>), Arr(Vec<Value>), Dict(BTreeMap<String, Value>), Ref(u32), Stream(BTreeMap<String, Value>, Vec<u8>) }
pub enum Seg { Line(P, P), Cubic(P, P, P, P) }        // P = (f64, f64) in pt
pub enum Item { Stroke { rgb: [f64; 3], width: f64, dash: Vec<f64>, subpaths: Vec<Vec<Seg>> },
                Fill { rgb: [f64; 3], subpaths: Vec<Vec<Seg>> },
                Text { rgb: [f64; 3], size: f64, matrix: [f64; 6], tz: f64, text: String } } // text WinAnsi-dekodiert (eigene Tabelle im Leser)
pub fn read(bytes: &[u8]) -> Result<Pdf, String>;   // prüft Kopf, nur ASCII, jedes xref-Versatz zeigt exakt auf „n 0 obj“, /Size, startxref, /Length exakt,
                                                    // Seitenbaum (/Count = Kids, Eltern), erlaubte Operatoren (J j w d RG rg G g m l c h re S f BT ET Tf Tm Td Tz Tj), `%%EOF\n` am Ende
pub fn utf16_text(v: &Value) -> String;             // <FEFF…> → Text
pub fn mm(pt: f64) -> f64;
```

- [ ] **Step 1: Failing tests (`stage26_pdf.rs`, Blätter von Hand gebaut über `SheetModel { sheet, frame, views }` und `common::svg_marks::manual_sheet`):**
  (a) `file_structure_is_exact` (1 und 3 Seiten: beginnt mit `%PDF-1.4\n`, alle Bytes < 0x80, endet mit `%%EOF\n`, `read` gelingt, Objektnummern nach P1, Kids in Eingabereihenfolge, `/Count 3`; der Leser beißt: eine Kopie mit einem eingefügten Byte vor Objekt 3 und eine mit `/Length` + 1 werden abgelehnt);
  (b) `page_sizes_in_points` (A4 hoch `[0 0 595.2756 841.8898]`, A4 quer `[0 0 841.8898 595.2756]`, A3 quer `[0 0 1190.5512 841.8898]`, A0 hoch `[0 0 2383.937 3370.3937]`; alle 10 Blätter `fits_page_pt`; `fits_page_pt(14400.0, 1.0)` wahr, `(14400.0001, 1.0)` falsch);
  (c) `info_and_identity` (Titel „Grundplatte ÄÖÜ ß Ø 😀“, Ersteller „Prüfer“, Datum „2026-10-18“ → Info liest sich zurück, `/CreationDate (D:20261018000000Z)`, `/Producer` und `/Creator` „PlanoraCAD“; `author: None` → kein `/Author`; Titel leer → kein `/Title`; Datum „18.10.2026“, „2026-13-01“, „2026-10-32“ → kein `/CreationDate`; `pdf_bytes` zweimal byte-gleich; anderer Titel → Inhaltsströme gleich, nur Info und Versätze anders; kein `/ID` in der Datei);
  (d) `view_lines_widths_and_dashes` (sichtbare Strecke (10,10)–(110,10), verdeckte (10,20)–(110,20): zwei Striche; 1. `width` 1.4173, `dash` leer, Teilpfad `m 28.3465 28.3465 l 311.811 28.3465`; 2. `width` 0.7087, `dash [8.5039 2.8346]`; Farbe schwarz; ohne verdeckte Linien kein zweiter Strich);
  (e) `mark_pens` (A3: `Dimension` 0.7087 ohne Muster, `CenterLine` 0.7087 mit `[34.0157 8.5039 5.6693 8.5039]`, Rahmen `Heavy`/`Medium`/`Thin` 1.9843/1.4173/0.7087, `Position(Circle)` 0.9921, `Position(Leader)` 0.7087, `Lost` mit `rgb [0.8, 0, 0]`; A4 (Faktor 0,7071): Rahmen 1.4031/1.0022/0.5102 (Mindeststärke 0,18 mm));
  (f) `arrows_are_filled_triangles` (Spitze (50,50), Richtung (1,0): eine Fläche, kein Strich, Ecken (50,50), (53.5,50.6), (53.5,49.4) in mm ±1e-4);
  (g) `text_anchor_and_rotation` („Ä“ Mitte bei (100,100), h 3,5 → `Tf 9.9213`, `Tm [1 0 0 1 280.1558 283.4646]`; „10“ Ende, 90°, bei (50,50) → `Tm [0 1 -1 0 141.7323 130.6998]`; Anfang bei (20,30) → `e 56.6929, f 85.0394`; mit der Leser-Breitentabelle zurückgerechnet liegt der Ankerpunkt jedes Texts auf ±1e-4 mm am Sollpunkt);
  (h) `text_compressed_never_wider` (`Sheet`, h 10, „@@@@“ → `Tz 93.5961` vor und `100 Tz` nach dem `Tj`; „WWW“ → kein `Tz`; `Dimension`, h 3,5, „Ø20“ → `Tz 95.2381`; für jedes zulässige Zeichen c als `Sheet`-Text h 5: gerenderte Breite ≤ `sheet_text_width(c, 5) + 1e-9`; als `Dimension`-Text ≤ `text_width(c, 5) + 1e-9`);
  (i) `text_bytes` (`(a)\` → `\(a\)\\`; „äÖß€…–“ → `\344\326\337\200\205\226`; „−0,1“ → `(-0,1)`; Leser dekodiert zurück; ganze Datei ASCII);
  (j) `curves_as_beziers` (Vollkreis r 10 um (100,100): ein Teilpfad, 4 `c`, letzter Punkt = erster exakt; Viertelbogen 0°–90°: Steuerpunkte (110,100), (110,105.5228), (105.5228,110), (100,110) ±1e-4 mm; Bogen 100° → 2 Stücke; Bogen über 0° (−30°…30°) → 1 Stück, Endpunkte wie SVG; negativer Radius → dieselben Endpunkte wie `svg_string` des Bogens; Ellipse `major (20,0)`, `ratio 0.5`, Viertel → Punkte auf der Ellipse, Abweichung ≤ 3e-4·20 mm an 50 Stellen; Spanne 1e-9 → 1 Stück, endliche Zahlen; `Mark::Arc` mit r 0 → kein Element; Spline Grad 1 → nur `l`; Grad 2 → genau die angehobenen Steuerpunkte; Grad 3 → dieselben Steuerpunkte; rationaler Viertelkreis Grad 2 → Abweichung ≤ 1e-6 mm an 50 Stellen);
  (k) `bad_input_writes_nothing` (NaN in einer Strecke → `Err(NonFinite)`, `out` leer; „😀“ und „Ā“ in einem Rahmentext → `Err(Unsupported(pdf_char_message(c)))`; 201 Seiten → `TOO_MANY_PAGES`, 200 Seiten gelingen; 0 Seiten → `NO_PAGES`);
  (l) `numbers` (`pdf_num(-0.00001) == "0"`, `1.23456 → "1.2346"`, `2.0 → "2"`, `1e-5 → "0"`, `12345678.9 → "12345678.9"`, kein `e` in der Ausgabe; NaN/∞ → `Err`).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage26_pdf -j 2`
- [ ] **Step 3: Implement** `pdf.rs` nach P1–P8 und den Interfaces; `fitted_cubics` aus `svg.rs` freigeben (keine Verhaltensänderung). Leser `pdf_read.rs` unabhängig vom Schreiber (eigene WinAnsi-Tabelle, eigene Zahlenlogik).
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-drawing -j 2` (SVG/DXF-Golden-Dateien unverändert), clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: xref-Eintrag mit 19 Bytes; ein Versatz +1; `/Length` + 1; `MediaBox` in mm; Muster nicht in pt; verdeckt 0,5; Mitte mit k = 1; Drehsinn von `Tm` vertauscht; `100 Tz` fehlt; Stauchung nie; `−` nicht auf `-`; Oktal als Dezimal; Kids umgekehrt; Titel als PDFDoc-String; `CreationDate` ohne Prüfung; NaN-Prüfung entfernt; Stücke bis 180°; κ mit `tan(h/2)`; Gradanhebung mit ⅓.
- [ ] **Step 6: Commit** — `feat: PDF-Schreiber mit Seiten, Linien, Bögen und Helvetica-Texten` (Pfade einzeln: `pdf.rs`, `svg.rs`, `lib.rs`, `tests/common/pdf_read.rs`, `tests/common/mod.rs`, `tests/stage26_pdf.rs`).

### Task 3: Gleichheit mit SVG, Golden-Dateien, Dateiausgabe

**Files:**
- Modify: `crates/planora-drawing/src/file.rs` (`DrawingFormat::Pdf` mit `extension() == "pdf"`, `label() == "Zeichnung als PDF"`; `drawing_to_bytes(m, Pdf) = pdf_bytes(&[m], &PdfMeta::default())`; `write_via_temp` P14; `export_pdf_file`), `crates/planora-drawing/src/lib.rs` (`pub use file::export_pdf_file`), `.gitattributes` (Zeile `crates/planora-drawing/tests/fixtures/*.pdf -text` – sonst verdirbt `core.autocrlf=true` Bytes und Versätze)
- Create: `crates/planora-drawing/tests/fixtures/pdf_cylinder_a4_landscape.pdf`, `crates/planora-drawing/tests/fixtures/pdf_box_frame_a3_portrait.pdf`
- Test: `crates/planora-drawing/tests/stage27_pdf_svg.rs` (neu); `crates/planora-drawing/tests/stage17_file.rs` (neue Fälle für PDF, bestehende unverändert)

**Interfaces:**
- Consumes: 1, 2; `common::{svg_read, svg_marks::{box_sheet, cylinder_sheet, manual_sheet}}`, `sheet_frame::{sheet_layout_with, PartsSpace}`, `sheet_marks::sheet_frame`, `balloon::layout_balloon`, `planora_exchange::{Progress, temp_path}`.
- Produces:
```rust
pub enum DrawingFormat { Svg, Dxf, Pdf }
pub(crate) fn write_via_temp(path: &Path, bytes: &[u8], progress: &Progress) -> Result<u64, DrawError>;
pub fn export_pdf_file(path: &Path, pages: &[&SheetModel], meta: &PdfMeta, progress: &Progress) -> Result<u64, DrawError>;
```
Golden-Blätter (Meta je `title "Golden"`, `author Some("Prüfer Ä")`, `date Some("2026-10-18")`): (1) `cylinder_sheet(true)` mit `sheet = A4 quer`, ohne Rahmen (Kreise, Ellipsen der Isometrie, Mittellinien); (2) `box_sheet(true)` mit `sheet = A3 hoch`, voller Rahmen aus `sheet_layout_with(A3 hoch, 2, 3, OnSheet { rows: 3 })` und Texten mit „Ø 12 ä ß µ ± ° € – −0,1“, drei Stücklistenzeilen, ein Nummernkreis (`layout_balloon`, Nummer 2) an der Ansicht „Vorne“.

- [ ] **Step 1: Failing tests (`stage27_pdf_svg.rs`):** (a) `same_content_as_svg` (für `box_sheet(true)`, `cylinder_sheet(true)`, Golden-Blatt 2, dasselbe auf A4 quer (skalierte Stifte), ein `manual_sheet` mit Maßpfeilen, gedrehten Texten (0°, 90°, 30°), einem verlorenen Maß und einem Spline-Blatt (Grad 1, 2, 3, 5 und rational): Zahl der Striche = Zahl der SVG-Pfade mit `stroke`, Zahl der Flächen = SVG-Pfade mit `fill`, Zahl der Texte gleich, in derselben Reihenfolge; je Element gleiche Teilpfadzahl, Anfangs- und Endpunkte ±1e-3 mm, Strichbreite ±1e-4 mm, Muster gleich; je Text Inhalt gleich (SVG-`−` als `-` verglichen), Ankerpunkt ±1e-3 mm, Höhe und Winkel gleich); (b) `beziers_follow_the_arcs` (alle Bögen von `cylinder_sheet` und Golden-Blatt 2: größte Abweichung der Bézier-Stücke vom exakten Bogen ≤ 3e-4·r + 1e-6 mm); (c) `golden_files` (beide Golden-Blätter byte-gleich mit den Fixturen; die Fixturen bestehen `pdf_read::read`; Erzeugen nur einmalig mit `PLANORA_UPDATE_PDF_GOLDEN=1`, im Commit begründet); (d) `golden_svgs_unchanged` (bestehende SVG/DXF-Golden-Dateien gleich – Schutz der `fitted_cubics`-Freigabe). In `stage17_file.rs`: (e) `pdf_format_and_file` (`DrawingFormat::Pdf.extension() == "pdf"`, `label() == "Zeichnung als PDF"`; `drawing_to_bytes(&m, Pdf) == pdf_bytes(&[&m], &PdfMeta::default())`; `export_pdf_file` mit 3 Seiten schreibt genau `pdf_bytes`, liefert die Länge, keine `.planora-tmp`; vorhandene Zieldatei bleibt bei NaN-Blatt und bei abgebrochenem `Progress` unverändert, keine Hilfsdatei).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-drawing --test stage27_pdf_svg --test stage17_file -j 2`
- [ ] **Step 3: Implement** nach Interfaces und P14; findet (a) eine Abweichung des Schreibers aus Aufgabe 2, wird sie hier im Schreiber behoben (nicht im Test gelockert). Golden-Dateien erst erzeugen, wenn (a), (b) grün sind.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-drawing -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Rahmen nach den Ansichten; Gruppe „Positionen“ ausgelassen; Mittellinien vor Maßen; verdeckter Pfad vor sichtbarem; Spline Grad 2 ohne Anhebung als zwei `l`; ein Bogenstück mehr als SVG; Hilfsdatei bei Fehler nicht gelöscht; umbenennen vor `sync_all` ohne Prüfung auf Abbruch; `label()` „Zeichnung als Pdf“; `drawing_to_bytes(Pdf)` mit Titel; ein Byte der Golden-Datei geändert; `.gitattributes`-Zeile fehlt (Prüfung: `git check-attr text` meldet `unset`).
- [ ] **Step 6: Commit** — `feat: PDF als Zeichnungsformat, gleich zur SVG-Ausgabe, mit Golden-Dateien` (Pfade einzeln, inkl. `.gitattributes` und beide Fixturen).

### Task 4: Export-Fenster – PDF für das gewählte Blatt

**Files:**
- Modify: `crates/planora-app/src/export_dialog.rs` (`ExportFormat::ALL` mit 8 Einträgen, PDF zuletzt; `PdfPages`; Zeichenprüfung P13 im Auftrag bei PDF; `PdfWriteFn`, `set_pdf_writer`; `write_drawing` schreibt PDF über `pdf_writer` mit `pdf_meta(.., Stamp::now().date)`; gemeinsamer Prüfhelfer für DXF/PDF-Feldtexte)
- Test: `crates/planora-app/tests/export_pdf.rs` (neu; bindet den Leser über `#[path = "../../planora-drawing/tests/common/pdf_read.rs"] mod pdf_read;` ein); anpassen: `tests/export_drawing.rs::format_list_has_seven_entries_in_order` → `format_list_has_eight_entries_in_order` (PDF an Stelle 8), `tests/export_step.rs:94` (`ALL.len() == 8`)

**Interfaces:**
- Consumes: `DrawingFormat::Pdf`, `export_pdf_file`, `PdfMeta`, `pdf_char_ok` (1–3); `prepare_drawing_parts`, `parts_input`, `FrameInput`, `title_field_name`, `context_or_single`.
- Produces:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PdfPages { #[default] Selected, All }
pub type PdfWriteFn = fn(&Path, &[&SheetModel], &PdfMeta, &Progress) -> Result<u64, DrawError>;
impl ExportDialog { #[doc(hidden)] pub fn set_pdf_writer(&mut self, f: PdfWriteFn); }
pub fn pdf_char_text(c: char, field: &str) -> String;     // Spec §4, wörtlich
pub fn pdf_sheet_texts_ok(texts: &TitleTexts) -> Result<(), String>;   // P13, Feldnamen wie DXF
pub fn pdf_model_texts_ok(model: &SheetModel) -> Result<(), String>;   // Texte der Ansichten, nennt die Ansicht
pub fn pdf_meta(state: &AppState, pages: PdfPages, drawing: Option<DrawingId>, today: &str) -> PdfMeta; // P10, P11
```

- [ ] **Step 1: Failing tests (`export_pdf.rs`, Hilfen wie in `export_drawing.rs`: `box_state`, `add_drawing`, `dialog`, `done`, `temp_dir`, `temp_files`):** (a) `pdf_is_the_eighth_format` (`ExportFormat::ALL[7] == Drawing(Pdf)`, Beschriftung „Zeichnung als PDF“, Endung „pdf“; die gerenderte Formatkarte zeigt den Eintrag; Karten bleiben gleich hoch bei 720 px); (b) `selected_sheet_is_written` (A3-quer-Zeichnung „Blatt 1“ mit vier Ansichten: Status `DrawingReady` wie bei SVG; Dateiauswahl schlägt „Blatt 1.pdf“ vor; geschriebene Datei == `pdf_bytes(&[&eigenes Blatt], &pdf_meta(..))`, `read` gelingt, 1 Seite `[0 0 1190.5512 841.8898]`, Titel „Blatt 1“, `CreationDate` aus dem Ausgabedatum des Blatts; Meldung „Fertig: „Blatt 1.pdf“ geschrieben (…)“; keine Hilfsdatei); (c) `frame_switch_applies` (Rahmen an: Texte „Blatt 1“ und das Ausgabedatum im Inhalt, Striche mit 1.9843; aus: keine Rahmentexte, nur Ansichtsstriche; Stücklistenblatt ohne Rahmen: nur Tabelle mit „Benennung“); (d) `char_lock_names_the_field` (Titel „Halter 😀“ → `Failed(pdf_char_text('😀', "Titel"))`, keine Datei; SVG derselben Zeichnung wird geschrieben; Benennung „Ā“ in der Stückliste → Feld „Stückliste, Pos. 1, Benennung“; eigener Maßtext mit „😀“ → Meldung nennt die Ansicht „Vorne“; Titel „Platte −0,1 … – Ø ± ° µ €“ → wird geschrieben, im Inhalt `-0,1`); (e) `lost_balloon_blocks_pdf` (gleiche Meldung `lost_balloon_text("Vorne")` wie SVG, keine Datei); (f) `extension_and_name` (`normalize_target(PDF, "x.PDF") == "x.pdf"`, `"x"` → `"x.pdf"`, `"x.svg"` → `"x.svg.pdf"`; Name mit „/“ → „_“); (g) `existing_file_stays_and_tab_close_leaves_nothing` (langsamer PDF-Schreiber über `set_pdf_writer`: Abbrechen → alte Datei bleibt, `Failed("Abgebrochen")`; Fenster lässt sich während des Schreibens nicht schließen; Tab schließen während des echten Schreibens → keine Hilfsdatei, Ziel alt oder fertige PDF – Muster `closing_a_tab_while_a_drawing_is_written_leaves_no_helper_file`); (h) `pdf_meta_rules` (Ersteller-Text „Erika“ in den Projektangaben → `author Some("Erika")`, `Auto` → `None`; eigenes Ausgabedatum „2026-01-02“ → dieses; „Anfang Mai“ → `info.created`; Zeichnung ohne Satz → `today`; `All` → Titel = Dokumentname, Datum vom ersten Blatt); (i) `formats_keep_their_own_results` (SVG fertig, Wechsel auf PDF rechnet eigenes Ergebnis mit `dxf: false`, zurück auf SVG ohne Neuberechnung; DXF-Meldungen von `dxf_sheet_texts_ok` wörtlich unverändert).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test export_pdf -j 2`
- [ ] **Step 3: Implement** nach Interfaces, P10, P11, P13 (der Auftrag prüft bei PDF nach `prepare_drawing_parts`: erst `pdf_sheet_texts_ok`, dann `pdf_model_texts_ok`).
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test export_drawing --test export_dimensions --test export_step --test export_dialog --test wide_dialogs --test no_hard_coded_colors --test fonts -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: PDF nicht in `ALL`; PDF über den SVG-Schreiber (`writer`); Rahmen mit `dxf: true`; Zeichenprüfung nur für Felder, nicht für Ansichten; Stückliste nicht geprüft; Feldname „Title“; `−` als unzulässig; Meta-Titel immer Dokumentname; Ersteller = Anmeldename; Datum immer `today`; Hilfsdatei bleibt bei Abbruch; Schließen während des Schreibens erlaubt.
- [ ] **Step 6: Commit** — `feat: Zeichnung als PDF im Export-Fenster` (Pfade: `export_dialog.rs`, `tests/export_pdf.rs`, `tests/export_drawing.rs`, `tests/export_step.rs`).

### Task 5: Export-Fenster – alle Blätter in einer PDF

**Files:**
- Modify: `crates/planora-app/src/export_dialog.rs` (Feld `pdf_pages`; Auswahl in der Karte nach P9; `DrawKey` als Struktur mit `target: DrawTarget { One(DrawingId), All }`; Auftrag für alle Blätter nach P12; Ergebnis als Liste von `DrawingPrepared`; Status-Summen; Fortschrittstext; `begin_save`/`pdf_meta` nach P10)
- Test: `crates/planora-app/tests/export_pdf.rs` (weitere Tests)

**Interfaces:**
- Consumes: 4; `Document::drawings`, `parts_input`, `context_or_single`, `MAX_PAGES`.
- Produces:
```rust
pub const PDF_SELECTED: &str = "Gewähltes Blatt";
pub const PDF_ALL: &str = "Alle Blätter";
pub const PDF_ALL_HINT: &str = "Bei „Alle Blätter“ kommen alle Zeichnungen der Reihe nach in die PDF.";
pub struct ExportDialog { /* … */ pub pdf_pages: PdfPages }           // Vorgabe Selected
pub fn sheet_progress_text(n: usize, total: usize) -> String;          // „Blatt {n} von {total} wird berechnet …“
pub fn in_sheet(sheet: &str, msg: &str) -> String;                     // „Im Blatt „{sheet}“: {msg}“
pub fn too_many_pages_text(n: usize) -> String; // „Eine PDF kann höchstens 200 Blätter enthalten, das Dokument hat {n}. Bitte einzelne Blätter ausgeben.“
pub fn pages_label(n: usize) -> String;                                // „1 Blatt“ / „{n} Blätter“
#[derive(Clone, Debug)] pub struct SheetJob { pub drawing: Drawing, pub frame: Option<FrameInput>, pub parts: PartsInput }
pub fn prepare_all_sheets(sheets: &[SheetJob], solids: &[(String, Solid)], compute: ComputeFn, resolve: ResolveFn,
                          counter: &AtomicUsize, progress: &Progress) -> Result<Vec<DrawingPrepared>, String>; // P12
impl ExportDialog { pub fn computing_sheet(&self) -> Option<(usize, usize)> }   // (n, N) während „Alle Blätter“ rechnet
```

- [ ] **Step 1: Failing tests (`export_pdf.rs`):** (a) `all_sheets_in_document_order` (Dokument mit „Halter“ (A3 quer), „Deckel“ (A4 hoch), Stücklistenblatt mit 45 Zeilen auf A4 hoch (2 Seiten) dazwischen angelegt: `pdf_pages = All` → eine Datei mit 4 Seiten in Dokumentreihenfolge, Seitengrößen je Blatt, Titel = Dokumentname, vorgeschlagener Name „{Dokumentname}.pdf“; Seite 4 enthält Pos. „45“; mit Rahmen „Blatt 1 von 4“ … „4 von 4“ im Inhalt; jede Seite == eigenes Blatt dieses Dokuments; `Selected` bleibt eine Seite); (b) `choice_ui` (die Wahl erscheint nur bei PDF, Vorgabe `PDF_SELECTED`; bei `PDF_ALL` Zeichnungsauswahl gesperrt mit `PDF_ALL_HINT`, Angaben „Blätter: 4“; SVG/DXF zeigen keine Wahl; Karten gleich hoch; 800 × 600 ohne Überlauf); (c) `one_broken_sheet_locks_everything` (verlorener Nummernkreis auf Blatt 3 von 5 → `Failed(in_sheet("Blatt 3", &lost_balloon_text("Vorne")))`, keine Datei, keine Hilfsdatei; zusätzlich Emoji auf Blatt 5 → weiterhin die Meldung von Blatt 3 (erster Fehler); verlorenes Maß, nicht berechenbare Ansicht (`fail_top`), Zeichen im Stücklisten-Folgeblatt 2 → je benannt mit Blatt; Kreis gelöscht → PDF mit 5 Seiten); (d) `blocked_before_computing` (Ansichtenzeichnung ohne Ansicht an Stelle 2 → `Unavailable` mit der vorhandenen Meldung; 201 Zeichnungen → `Unavailable(too_many_pages_text(201))`, 200 ohne diese Sperre); (e) `stale_and_key_coverage` (nach fertigem Ergebnis je einzeln: Zeichnung hinzufügen, eine löschen, eine umbenennen, Dokument umbenennen, Rahmen-Schalter, `Selected`↔`All`, andere Zeichnung in der Auswahl bei `All` (darf **nicht** neu rechnen) → `write_to` ergibt `STALE` bzw. bei der Auswahl das alte gültige Ergebnis; neue Datei mit neuer Seitenzahl/Reihenfolge); (f) `progress_names_the_sheet` (langsames `compute` wie `slow`: während des Rechnens `computing_sheet() == Some((1, 3))`, später `(2, 3)`; gerenderter Text `sheet_progress_text(2, 3)`; „Exportieren …“ gesperrt; „Abbrechen“ → `Failed("Abgebrochen")`, keine Datei); (g) `tab_close_during_all` (Tab schließen während „Alle Blätter“ rechnet bzw. schreibt → Auftrag endet, keine Hilfsdatei).
- [ ] **Step 2: Run → FAIL.** `cargo test -p planora-app --test export_pdf -j 2`
- [ ] **Step 3: Implement** nach Interfaces, P9, P10, P12.
- [ ] **Step 4: Run → PASS**, dazu `cargo test -p planora-app --test export_drawing --test export_dimensions --test export_dialog --test wide_dialogs --test parts_sheet --test dialogs -j 2`, clippy, fmt.
- [ ] **Step 5: Mutationsbeweise (≥ 10)**, z. B.: Blätter mit Fehler überspringen; letzten statt ersten Fehler melden; Meldung ohne Blattnamen; Reihenfolge nach `DrawingId` statt Dokument; Stücklistenblätter ausgelassen; Kontext immer „1 von 1“; `DrawKey` ohne `pdf_pages`; `All`-Schlüssel mit Zeichnungs-Id (Auswahl rechnet neu); 200er-Sperre ab 200; Zähler nie erhöht; Titel = Zeichnungsname bei `All`; Abbrechen nicht weitergereicht.
- [ ] **Step 6: Commit** — `feat: alle Blätter in einer PDF mit Sperre je Blatt`.

### Task 6: Proben, Dokumentation, Gesamtprüfung, Handliste

**Files:**
- Create: `crates/planora-drawing/tests/hand_samples_pdf.rs` (`#[ignore]`; Ziel `$PLANORA_PDF_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben\pdf`)
- Modify: `README.md` (Abschnitt Zeichnungen: PDF, gewähltes/alle Blätter, Schrift nicht eingebettet, westeuropäischer Zeichensatz; „Drittanbieter“: Helvetica-Breiten aus Adobe Core 14 AFM mit Copyright-Zeile und Bedingungen), `docs/superpowers/offene-punkte.md` (Abschnitt „Teilprojekt 3c-3 – PDF-Ausgabe“: Geliefertes, P1–P14, Grenzen: Schrift nicht eingebettet (Darstellung je Betrachter leicht anders), nur WinAnsi, keine Komprimierung, „Alle Blätter“ rechnet gleiche Ansichtsarten je Blatt neu (langsam bei vielen Blättern), Datum UTC, Stauchung bei breiten Zeichen), `docs/superpowers/specs/2026-10-18-pdf-ausgabe-design.md` (neuer §9 „Festlegungen der Umsetzung“: P1–P14 kurz, Antworten auf Q1/Q2), dieser Plan (Abschnitt „Abweichungen bei der Umsetzung“)

- [ ] **Step 1: Proben erzeugen** (`cargo test --release -p planora-drawing --test hand_samples_pdf -- --ignored`, Vordergrund): je mit und ohne Rahmen A4 hoch, A4 quer, A3 quer, A0 quer (Quader mit Bohrung, Maße mit „Ø“, „±“, „−0,1“, Mittellinien, Isometrie); 12 Körper mit Stückliste auf dem Blatt und Nummernkreisen (auch „200“); ein Stücklistenblatt mit zwei Seiten; eine mehrseitige PDF „alle-blaetter.pdf“ (6 Seiten gemischter Formate); zu jeder PDF die SVG daneben.
- [ ] **Step 2: Doku** in Laiensprache; **Handliste** für den Nutzer: PDF des gewählten Blatts mit/ohne Rahmen in Edge, Chrome und einem weiteren Betrachter öffnen (Seite in richtiger Größe/Ausrichtung, Texte in Zellen, Umlaute, Ø ° ± µ €, Linienstärken wie SVG, verdeckt gestrichelt, Mittellinien Strich-Punkt); „Alle Blätter“ mit Stücklistenblatt (Seitenzahl, Reihenfolge, „Blatt n von N“); A2/A0 auf A4 verkleinert drucken; Emoji im Titel → Meldung mit Feld und Blatt; verlorene Positionsnummer → Sperre; Fenster bei 800 × 600 und groß, Hell und Dunkel.
- [ ] **Step 3: Gates** mit Ausgabe: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace -j 2`; `cargo deny check`; `git diff --stat 4840dfe -- crates/planora-kernel crates/planora-mesh/src crates/planora-step crates/planora-exchange` leer; `git diff 4840dfe -- '**/Cargo.toml' Cargo.lock deny.toml` leer; `git check-attr text -- crates/planora-drawing/tests/fixtures/pdf_box_frame_a3_portrait.pdf` meldet `unset`; `git status` zeigt nur `assets/` unverfolgt.
- [ ] **Step 4: Mutationsbeweise:** entfällt für reine Doku; dafür die Summenliste aller Mutationen der Aufgaben 1–5 im Bericht (mindestens 50 Einträge).
- [ ] **Step 5: Commit** — `docs: PDF-Ausgabe (3c-3), Festlegungen, Handliste, Proben`.

### Task 7 (Controller): unabhängige Prüfung, Test im echten Fenster, Haltepunkt

- [ ] **Step 1: Unabhängige Prüfung der Proben und Golden-Dateien** mit Poppler (auf diesem Rechner unter `%LOCALAPPDATA%\Microsoft\WinGet\Packages\oschwartz10612.Poppler_…\poppler-25.07.0\Library\bin`): `pdfinfo` (Seitenzahl, Seitengröße in pt, Titel/Ersteller, keine Warnungen auf stderr), `pdftotext -layout` (Texte inkl. Umlaute, `Ø`, `−` als `-`), `pdftoppm -r 100 -png` (Bilder ansehen: Rahmen, Zellen, Linienstärken, Bögen glatt, nichts verrutscht); dazu je eine Probe in Edge öffnen (eingebauter Browser, `file://`) und ansehen. Befunde ins Ledger; Fehler gehen als Korrekturrunde an die zuständige Aufgabe.
- [ ] **Step 2 (nur mit Freigabe des Nutzers): eigener Test im echten Fenster** vor der Merge-Frage: Release bauen (Vordergrund, `-j 2`), App starten, mit `tools/gui-drive.ps1` (click/dclick/rclick/drag/wheel/type, Bildschirmfoto `%TEMP%\planora_shot.png`) Kernweg: Quader, Zeichnung mit Ansichten, Export-Fenster → „Zeichnung als PDF“ → gewähltes Blatt schreiben, „Alle Blätter“ schreiben (Fortschritt „Blatt n von N“ sichtbar), Emoji im Titel → Meldung; Bildschirmfotos ansehen, erzeugte PDF mit Poppler prüfen; App beenden, kein Prozess bleibt zurück.
- [ ] **Step 3: Haltepunkt:** Nutzer prüft Proben und Handliste; Gesamtreview des Zweigs, eine Korrekturrunde, Merge-Frage.

---

## Bestehende Tests und Golden-Dateien, die sich ändern

- **Formatliste:** `crates/planora-app/tests/export_drawing.rs::format_list_has_seven_entries_in_order` → acht Einträge (umbenannt in `format_list_has_eight_entries_in_order`), `crates/planora-app/tests/export_step.rs:94` (`ExportFormat::ALL.len() == 8`; `ALL[4] == Step` bleibt).
- **Erschöpfende `match` über `DrawingFormat`:** nur `file.rs` und `export_dialog.rs` (Code, keine Testerwartung).
- **Kartenhöhe/Fenstergröße:** Die Formatkarte bekommt eine Zeile mehr; `cards_stay_centred_720_with_equal_heights` und `wide_dialogs` müssen ohne neue Erwartungen grün bleiben – nur wenn eine Erwartung eine feste Zeilen- oder Pixelzahl nennt, wird sie mit Rechnung im Commit angepasst.
- **Unverändert (muss so bleiben):** `svg_without_marks_{box,cylinder}.svg`, `dxf_without_marks_{box,cylinder}.dxf`, `svg_balloons_a4_a0.svg`, `stage15_sheet_lines.txt`; alle DXF-Meldungen (`dxf_sheet_texts_ok`) wörtlich; Dateiformat 7 (PDF ändert nichts am Dokument).
- **Neu:** Golden-Dateien `pdf_cylinder_a4_landscape.pdf`, `pdf_box_frame_a3_portrait.pdf`; `.gitattributes`-Zeile für `*.pdf`.

## Self-Review (vom Plan-Verfasser)

**Spec-Abdeckung:** §1/§2 Ziele: Format → 3, 4; gewählt/alle → 4, 5; Rahmen-Schalter → 4 (c), 5 (a); Helvetica nicht eingebettet, WinAnsi, benannte Meldung → 1, 2 (k), 4 (d); ohne Bibliothek → Global Constraints, Gate 6. Nicht-Ziele unberührt (keine Einbettung, Lesezeichen, Komprimierung, Bilder, Druck). Anti-Ziele: Betrachter → 2 (a), 3 (c), 7; Text aus der Zelle → P6, 2 (h); Seitengröße/Ausrichtung → 2 (b), 5 (a); Linienstärke → 2 (d, e); passt zu SVG → 3 (a); keine stillen Auslassungen und keine Ausgabe verlorener Kreise → 4 (e), 5 (c). §3 Seite = Blatt, Inhalt, Quelle, Bögen ≤ 90°, Texte, Mehrseitig, Eigenschaften, Struktur → P1–P7, 2, 3, 5. §4 Bedienung, Dateiname, Meldungen, Fortschritt → P9–P13, 4, 5. §5 Grenzen, Rundung, nicht endlich, Hilfsdatei → P2, P8, P14, 2 (k), 3 (e), 4 (g). §6 Selbsttest/Inhaltsleser → 2; Äquivalenz → 3 (a); Byte-Gleichheit, Golden A4 quer/A3 hoch → 2 (c), 3 (c); Zeichenmetrik → 1, 2 (g, h); Export-Fenster → 4, 5; echte Betrachter und Proben → 6, 7; geschützte Pfade → Gate 6; echtes Fenster → 7. §8 F1 → P6, 2 (h) und Q1; F2 → P12, 5 (c), (f).
**Schritt-Scan:** Testschritte nennen Namen, Eingaben und Zahlen (pt-Werte nachgerechnet: 72/25,4 = 2,834645669…); keine Rümpfe außer festem Dateiaufbau (P1) und Wortlauten.
**Typkonsistenz:** `winansi_byte`, `pdf_char_ok`, `helvetica_width`, `pdf_text_width` (1) → 2, 4; `PdfMeta`, `pdf_bytes`, `write_pdf`, `pdf_char_message`, `PT_PER_MM`, `MAX_PAGES` (2) → 3–5; `DrawingFormat::Pdf`, `export_pdf_file`, `write_via_temp` (3) → 4, 5; `PdfPages`, `PdfWriteFn`, `pdf_meta`, `pdf_char_text`, `pdf_sheet_texts_ok`, `pdf_model_texts_ok` (4) → 5; `SheetJob`, `prepare_all_sheets`, `in_sheet`, `sheet_progress_text` (5).
**Proportion:** Entscheidungen, Zahlen, Schnittstellen, Tests; kein Funktionsrumpf.
**Bekannte Risiken:** Aufgabe 2 ist die größte (Schreiber und Leser); Aufgabe 5 baut den Schlüssel des Zeichnungsergebnisses um (viele bestehende Export-Tests laufen dadurch mit). Empfehlung Controller: opus für 2, 3, 5; sonnet für 1, 4, 6.

**Offene Fragen an den Nutzer (ändern, was gebaut wird):**
- **Q1 – Stauchung breiter Texte (P6):** F1 nimmt an, die echte Helvetica sei immer schmaler als die Schätzung. Für einzelne Zeichen stimmt das nicht (`@` 1,015 statt 0,95; `O`, `Q`, `G` 0,778 statt 0,72; `—`, `…`, `‰`, `™` 1,0 statt 0,6; `&` 0,667 statt 0,6). Ein bis an den Rand gefüllter Eintrag aus solchen Zeichen ragte dann bis rund 8 % (bei `…`/`—` mehr) über seine Zelle. Der Plan staucht in diesem seltenen Fall den Text waagerecht (Inhalt unverändert). Einverstanden, oder soll die PDF solche Texte wie SVG unbearbeitet lassen?
- **Q2 – Ersteller in den PDF-Eigenschaften (P11):** Nur ein in den Projektangaben eingetragener Ersteller kommt in die Dateieigenschaften, nicht der automatische Windows-Anmeldename (der im Schriftfeld steht). Einverstanden, oder auch den Anmeldename übernehmen?

## Abweichungen bei der Umsetzung

(wird nach der Umsetzung gefüllt)

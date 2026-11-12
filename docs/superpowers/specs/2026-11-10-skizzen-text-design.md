# Skizzen 8: Text

Stand 2026-11-10. Achtes von zehn Skizzen-Teilprojekten. Maßstab ist Fusions Verhalten (Dennis: „Es soll die gleichen Funktionen wie in Fusion haben“). Dennis hat eine Frage selbst entschieden: Fehlt eine Schrift, wird sofort mit einer Ersatzschrift neu gesetzt, wie in Fusion. Alles andere hat Claude nach Fusion festgelegt. Was nur aus dem Gedächtnis oder aus Foren stammt, ist als **Annahme** markiert und kommt in `offene-punkte.md`.

## Warum

Beim 3D-Druck steht oft Schrift auf dem Teil, etwa Beschriftungen, Namen, Nummern oder Text um einen Knopf herum. In Fusion schreibt man den Text in die Skizze, extrudiert ihn und ändert ihn später einfach. Heute fehlt das ganz. Skizzen 7 hat die Kurven gebracht, die Buchstaben genau darstellen: Kegelschnitt mit Rho 0,5 als quadratische Bézierkurve, Kontrollpunkt-Spline Grad 3 als kubische.

## Was Fusion tut

Aus der Autodesk-Hilfe (Text-Dialog en/de, „Create text“, „Explode Text“) und der API (`SketchTextInput`, `MultiLineTextDefinition`, `AlongPathTextDefinition`, `TextStyles`). Forenwissen ist gekennzeichnet.

- **Typen im Dialog:** „Text“ und „Text auf Pfad“ (en „Text“, „Text On Path“). „An Pfad anpassen“ (en „Fit to Path“) ist ein Häkchen bei „Text auf Pfad“.
- **Felder:** Text (eine oder mehrere Zeilen), Pfad (nur auf Pfad), Schriftart, Schrift (Fett, Kursiv), Höhe, Zeichenabstand, Platzierung oberhalb/unterhalb (nur auf Pfad), An Pfad anpassen, Umkehren waagerecht/senkrecht, Ausrichtung waagerecht (links, zentriert, rechts) und senkrecht (oben, Mitte, unten; nur bei „Text“).
- **Unterstrichen** gibt es nicht (in der API vorhanden, aber „not currently supported“). Es entfällt hier.
- **Rahmen:** zwei Klicks, erste Ecke und Gegenecke. Der Rahmen besteht aus vier echten Skizzenlinien. Abhängigkeiten und Maße daran steuern Lage, Größe und Winkel. An den Buchstaben selbst gehen keine Maße.
- **Winkel:** im Dialog kein Feld. Man dreht über einen Drehgriff oder gibt einen Winkel ein. Auf dem Kreis dreht man mit „Verschieben/Kopieren“ um die Mitte.
- **Zeichenabstand:** in Prozent zusätzlich zum Abstand der Schrift, Vorgabe 0. Negative Werte lehnt Fusion ab (Forum).
- **Auf Pfad:** jede Skizzenkurve. Die Ausrichtung bezieht sich auf den Pfad. „An Pfad anpassen“ spreizt den Abstand, bis der Text die ganze Länge füllt. Die Höhe bleibt dabei gleich. Ändert sich der Pfad, folgt der Text.
- **Auflösen:** Kontextmenü „Text auflösen“. Der Text verschwindet, übrig bleiben feste Skizzenkurven (Forum: Linien und Splines, mit Fixierung). Das lässt sich nicht als Text zurückholen.
- **Profile:** Text ist direkt ein Profil für Extrudieren, mit Innenkonturen. Sich überlappende Buchstaben (Schreibschriften) machen in Fusion Ärger (Forum).
- **Fehlende Schrift:** Fusion ersetzt still (Forum: Arial oder Calibri), ohne Hinweis. Hier gibt es bewusst einen Hinweis (Auftrag).
- **Ändern:** Doppelklick auf den Text öffnet den Dialog wieder.
- **Zeilenabstand:** nicht einstellbar.
- **Parameter:** Fusion legt Text- und Höhenparameter an, `{param}` im Text setzt Werte ein. Das ist hier **Nicht-Ziel**.

## Bestand (geprüft im Code)

- `Sketch` (`planora-sketch/src/model.rs`) hält `points`, `curves`, `constraints`, `patterns` in Arenen. Löschen setzt `alive = false`, Ids sind Positionen und bleiben stabil.
- **Nächste Verwandte: Muster** (`pattern.rs`). Die Kopien sind eine reine Funktion aus Quelle und Werten. Für den Löser sind ihre Variablen Konstanten, `refresh_patterns` schreibt sie vor und nach jedem Lösen neu. Doppelklick öffnet das Muster wieder (`begin_pattern_edit_item`). Das Muster-Werkzeug schreibt bis OK nichts in die Skizze (`pattern_preview`, `finish_pattern`).
- **Projektionen** halten ihre Kurven mit `Fixed` fest und erneuern sie in `compute_sketch` → `reproject_curve`, mit gleichen Ids. Ein verlorener Bezug ist eine Warnung (`feature-sketch-projection-lost`).
- **Profile** (`profile.rs`, `arrangement.rs`) können Innenkonturen (`ProfileRef { outer, holes }`). Features merken sich Profile über `PieceRef` mit Kurven-Id und Parametern. **Darum müssen Textkurven ihre Ids behalten**, wenn der Text neu gesetzt wird. Der Schnitt aller Kurvenpaare kostet O(n²) mit Hüllquader-Vorfilter. Gemessen sind 200 Kurven unter 50 ms (`profile_perf.rs`). Mehrere hundert Buchstabenkurven sind ungemessen.
- **Exakte Kurven:** Kegelschnitt mit Rho 0,5 ist die quadratische Bézierkurve (`conic_nurbs`, Gewicht 1). Ein Kontrollpunkt-Spline Grad 3 mit 4 Punkten ist die kubische (`control_spline_layout(3, 4)`). Die öffentlichen `add_conic` und `add_control_spline` lehnen Sonderfälle ab (Scheitel auf der Sehne, doppelte Nachbarn). Die `pub(crate)`-Wege `add_conic_as` und `add_control_spline_as` tun das nicht.
- **Rückgängig:** `observe_sketch` nimmt eine Änderung der Skizze als einen Schritt, Beschriftung über `edit_label`. Abbruch über `SketchSession::cancel` und `set_tool`.
- **Skizzenfeld** (`sketch_panel.rs`) trägt die Felder der Werkzeuge samt OK/Abbrechen (Vorbild `pattern_options`). Ein modaler Rahmen (`dialog.rs`) gehört zu den Feature-Dialogen.
- **Leiste:** genau 20 Knöpfe (`STRIP_TOOLS`, 813 px), feste Reihenfolge `TOOL_ORDER`. Menü Erstellen: `sketch_create_entries` mit `SKETCH_CREATE_ACTIONS`. Ändern-Menü: `sketch_modify_menu`. Tasten: `action_table!` in `keymap/defaults.rs`.
- **Format 17** (`convert.rs`). `required_format` nimmt das kleinste Format, das der Inhalt braucht. Neue Felder stehen mit `skip_serializing_if` in der Datei, dadurch bleiben alte Dateien bytegleich.
- **Schriften:** `skrifa` 0.44 (MIT OR Apache-2.0), `read-fonts` 0.41 und `harfrust` 0.12 (MIT) sind über egui schon im Lockfile. In unserem Code nutzt sie niemand direkt (`skrifa` ist nur Test-Abhängigkeit der App). Die App liefert Montserrat Regular, Medium, SemiBold (OFL 1.1) und Roboto (Apache-2.0) in `planora-app/assets/fonts` mit, samt README mit Prüfsummen.

## Lizenzen (geprüft)

- **Keine neue Fremd-Bibliothek.** Lesen, Umrisse und Setzen (Kerning, Ligaturen) gehen mit `skrifa` und `harfrust` in denselben Versionen, die egui schon zieht. Sie werden direkte Abhängigkeiten des neuen Crates `planora-text`. `cargo deny check` muss sauber bleiben, ohne neue Ausnahme.
- **Systemschriften, nichts mitgeliefert außer dem Vorhandenen.** Die Schriftliste zeigt die installierten Schriften und dazu Montserrat und Roboto aus der App. Neue Schriftdateien kommen nicht dazu.
- **In die Datei kommt nie eine Schriftdatei**, nur der Familienname und Fett/Kursiv. Die erzeugten Kurven stehen als normale Skizzenkurven in der Datei, weil die Profil-Verweise ihre Ids brauchen. Das ist berechnete Geometrie wie bei jedem STEP-Export eines extrudierten Textes, keine Schrift.
- **Ersatzschrift ist Montserrat** (OFL 1.1, liegt schon bei). Kein Autodesk-Material, auch keine Symbole.

## Ziele

### A. Schriften lesen und setzen (`planora-text`, neues Crate)

1. **Schriftliste:** einmal je Programmlauf, im Hintergrund ab dem Programmstart (siehe G4), werden die Schriftordner durchsucht (`.ttf`, `.otf`, `.ttc`, `.otc`, auch in Unterordnern). Windows: `%WINDIR%\Fonts` und `%LOCALAPPDATA%\Microsoft\Windows\Fonts`. Linux: `/usr/share/fonts`, `/usr/local/share/fonts`, `~/.local/share/fonts`, `~/.fonts`. Aus der Namenstabelle kommen Familie und Schnitt (Name-ID 16/17 vor 1/2), aus `OS/2` Gewicht und Kursiv. Kaputte Dateien werden still übersprungen und protokolliert. Kein eigener Schriftenzwischenspeicher auf der Platte. Tests geben ihre Ordner selbst vor und hängen nie an Systemschriften.
2. **Schnitt wählen:** Familie plus Fett/Kursiv → passendster Schnitt (Gewicht 700 und kursiv vor allem anderen). Bei variablen Schriften wird die Achse `wght` auf 700 gestellt, wenn Fett gewählt ist. Fehlt ein Schnitt: **kursiv nachgebildet** durch Scheren um 12° (`tan ≈ 0,2126`), **fett nachgebildet** durch Verbreitern der Umrisse um 2 % der Höhe nach dem Verfahren von FreeType (`FT_Outline_Embolden`: Kontrollpunkte entlang der gemittelten Normalen). **Annahme**, Fusion verlässt sich auf das Betriebssystem.
3. **Fehlende Familie:** Montserrat in passendem Schnitt (Fett → SemiBold, Kursiv nachgebildet). Der gespeicherte Name bleibt der ursprüngliche. Auf dem Rechner mit der Schrift sieht der Text also wieder wie vorher aus.
4. **Setzen:** `harfrust` mit der Schrift, Schreibrichtung aus dem Text (nur waagerecht, links nach rechts und rechts nach links wie vom Shaper geliefert). Ergebnis: je Zeile eine Folge von Glyphen mit Lage und Vorschub in Einheiten der Höhe. Kerning und Ligaturen gelten. Zeichen, die die Schrift nicht hat, werden mit Roboto gesetzt. Fehlen sie auch dort, bleibt der Platz leer und es gibt einen Hinweis „Zeichen fehlen“.
5. **Umrisse:** `skrifa`-Umrisse je Glyphe als geschlossene Konturen aus Strecken, quadratischen und kubischen Bézierkurven. Versteckte Punkte auf der Kurve (TrueType) werden ausdrücklich. Eine Strecke der Länge 0 entfällt. Eine Bézierkurve mit Kontrollpunkten auf einer Geraden wird zur Strecke, wenn die Abweichung unter `1e-9` × Höhe liegt.
6. **Höhe** ist die Geviertgröße (units per em) der Schrift. **Annahme**, so tun es die meisten Programme. Großbuchstaben sind damit etwa 70 % der Höhe.
7. **Zeichenabstand** in Prozent, `0 ≤ p ≤ 1000`, Vorgabe 0. Jeder Vorschub außer dem letzten in der Zeile wird um `p %` seines eigenen Werts länger. **Annahme** zur Bezugsgröße.
8. **Zeilen:** Enter im Textfeld bricht die Zeile. Zeilenabstand ist der der Schrift (`ascender − descender + lineGap` aus `hhea`, sonst `OS/2`), nicht einstellbar wie in Fusion. Kein automatischer Umbruch am Rahmen.
9. **Grenzen:** höchstens 500 Zeichen und 4000 erzeugte Kurven je Text. Darüber wird mit Meldung abgelehnt.
10. `planora-text` kennt weder egui noch die Skizze. Es liefert `TextShape` (Zeilen, Glyphen, Konturen in Einheiten der Höhe, Kennzahlen, Hinweise auf Ersatz oder fehlende Zeichen). Die Skizze legt die Kurven daraus an.

### B. Text in der Skizze (`planora-sketch`)

1. **Neues Objekt `TextData`** in einer eigenen Arena `texts` (wie `patterns`):
   - `content: String`, `font: String` (Familienname, wie gewählt), `bold`, `italic`, `height` (mm, intern), `spacing` (Prozent), `flip_h`, `flip_v`.
   - `placement`:
     - `Box { lines: [CurveId; 4], corners: [PointId; 4], h_align, v_align }`
     - `Path { path: CurveId, above: bool, fit: bool, h_align }`
   - `curves: Vec<CurveId>` (erzeugte Kurven, in fester Reihenfolge), `points: Vec<PointId>` (ihre Punkte), `alive`.
   - Laufzeitfeld ohne Datei: das zuletzt gesetzte `TextShape`.
2. **Erzeugte Kurven** sind echte Skizzenkurven: Strecke → `Line`, quadratisch → `Conic` mit ρ = 0,5 (über `add_conic_as`), kubisch → `ControlSpline` Grad 3 (über `add_control_spline_as`). Aufeinanderfolgende Stücke einer Kontur teilen den Endpunkt. Für den Löser sind ihre Punkte Konstanten wie bei Musterkopien. `refresh_texts` schreibt sie vor und nach jedem Lösen neu aus `TextShape` und der Lage von Rahmen oder Pfad. Sie sind keine Konstruktion, damit sie Profile bilden.
3. **Gleiche Ids beim Neusetzen:** Die Kurven werden der Reihe nach wiederverwendet, wenn die Art passt. Sonst wird die Kurve gelöscht und eine neue angehängt. Überzählige werden gelöscht, fehlende angehängt. Ändert sich nur Lage, Winkel, Höhe, Abstand oder Ausrichtung, behalten alle Kurven ihre Ids. Dann bleiben Extrusionen an ihrem Profil. Ändert sich der Inhalt oder die Schrift, sucht `profile_ref` wie heute nach dem besten Gegenstück.
4. **Rahmen:** vier Konstruktionslinien zwischen vier Eckpunkten. Die internen Abhängigkeiten halten sie als Rechteck: ein `Perpendicular` und zwei `Parallel` wie beim Rechteck über drei Punkte (5 Freiheitsgrade), **ohne** waagerecht oder senkrecht, damit er sich drehen lässt. Die erste Linie (Ecke 0 → Ecke 1) ist die Grundlinie, ihre Richtung ist der Winkel des Textes. Die Linien sind normale Linien für Maße, Abhängigkeiten und Fang. Ein neuer Rahmen bekommt `Horizontal` auf der Grundlinie, wie ein Rechteck. **Annahme**: In Fusion ist der Rahmen nach dem Aufziehen waagerecht.
5. **Lage im Rahmen:** Waagerecht links, zentriert, rechts an der Grundlinie, senkrecht oben, Mitte, unten zwischen Grundlinie und Gegenlinie. Bezug oben ist die Oberlänge (`ascender`) der ersten Zeile, unten die Unterlänge (`descender`) der letzten Zeile, Mitte die Mitte dazwischen. **Annahme.** Der Text darf über den Rahmen hinausragen.
6. **Umkehren:** waagerecht spiegelt den Text an der senkrechten Mittelachse des Rahmens, senkrecht an der waagerechten. Bei „auf Pfad“ kehrt waagerecht die Leserichtung um, senkrecht die Seite. **Annahme**, so wirkt es in Fusion.
7. **Auf Pfad:** jede Kurve außer Text- und Griffkurven, auch Konstruktion, auch projizierte (Linie, Kreis, Bogen, Ellipse, Ellipsenbogen, alle Splines, Kegelschnitt, fester Spline).
   - Jede Glyphe bleibt starr und wird an ihrer Mitte auf der Grundlinie an den Pfad gesetzt (Bogenlänge), gedreht nach der Tangente dort. Buchstaben werden nicht verbogen, wie in Fusion.
   - **Leserichtung (Annahme):** offene Kurven von Anfang nach Ende, Kreis und Bogen im Uhrzeigersinn. So steht Text oben auf einem Kreis aufrecht und außen. Oberhalb heißt links der Leserichtung, unterhalb rechts. Unterhalb liegt die Oberlänge an der Kurve, oberhalb die Grundlinie.
   - **Ausrichtung auf offener Kurve:** links am Anfang, zentriert an der Mitte der Länge, rechts am Ende. **Auf geschlossener Kurve:** Bezugspunkt beim Kreis ganz oben (90°), sonst am Kurvenanfang. Links beginnt dort, zentriert sitzt mittig darüber, rechts endet dort. **Annahme.**
   - **Über das Ende hinaus:** auf offener Kurve geht es auf der Tangente am Ende weiter, auf geschlossener läuft es weiter um die Kurve. **Annahme.**
   - **An Pfad anpassen:** Die Zwischenräume werden gleichmäßig verteilt, bis der Text vom Anfang bis zum Ende reicht. Auf geschlossenen Kurven ist der Abstand zwischen letzter und erster Glyphe genauso groß. Ist der Text länger als der Pfad, werden die Zwischenräume kleiner, auch unter 0, und Buchstaben dürfen sich überlappen. Ausrichtung und Zeichenabstand sind dann ohne Wirkung und im Feld gesperrt, wie in der API.
   - Mehrere Zeilen auf Pfad: Die Zeilen werden aneinandergehängt, mit einem Leerzeichen dazwischen. **Annahme**, Fusion ist hier nicht belegt.
   - Ändert sich der Pfad, folgt der Text beim nächsten Lösen.
8. **Gelöscht wird nur als Ganzes.** Löschen eines Textes löscht Rahmen, Eckpunkte, erzeugte Kurven und die Abhängigkeiten daran. Das ist ein Rückgängig-Schritt. Löschen einer Rahmenlinie, einer Ecke oder einer Textkurve allein wird mit Meldung abgelehnt („Gehört zum Text. Wähle den ganzen Text, um ihn zu löschen.“). Ein Pfad, an dem Text hängt, lässt sich nicht löschen. Meldung: „Text hängt an diesem Pfad. Löse den Text auf oder lösche ihn zuerst.“ **Annahme**: Fusions Verhalten ist nicht belegt, aber so geht nichts still verloren.
9. **Auswahl:** Ein Klick auf eine Textkurve wählt den ganzen Text, also alle Kurven, und zeigt den Rahmen hervorgehoben. Rahmenlinien und Ecken sind einzeln wählbar wie andere Linien. Rahmenwahl mit dem Rechteck wählt den Text, wenn eine seiner Kurven dabei ist. Textkurven bieten keine Fangpunkte. Fang am Rahmen geht wie bei Linien. **Annahme.**
10. **Konstruktion umschalten** am Text schaltet alle erzeugten Kurven um (keine Profile mehr), wie `pattern_style_group`. Mittellinie wird abgelehnt.

### C. Bedienung (`planora-app`)

1. **Befehl „Text“** (en „Text“) im Menü Erstellen über `SKETCH_CREATE_ACTIONS`, wie in Fusion nicht in der Leiste. Die Leiste bleibt bei 20 Knöpfen und 813 px. Belegbare Aktion `skizze.text`, ohne Vorgabetaste (**Annahme**: Fusion hat keine).
2. **Ablauf:** Der Befehl startet mit „Text“ als Art und wartet auf zwei Klicks für den Rahmen. Fang und vorhandene Punkte gelten. Ein Klick auf einen vorhandenen Punkt setzt eine `Coincident`-Abhängigkeit, wie bei Rechtecken. Danach öffnet sich der **Abschnitt „Text“ im Skizzenfeld**. Stellt man die Art auf „Text auf Pfad“ um, wartet das Werkzeug auf einen Klick auf eine Kurve. Ein zuvor gezogener Rahmen bleibt dabei nur Vorschau.
3. **Felder** in Fusions Reihenfolge:
   - Art: Text, Text auf Pfad
   - Text: mehrzeilig, Enter für eine neue Zeile, Strg+Enter für OK
   - Pfad: nur auf Pfad, zeigt die gewählte Kurve, Klick wählt neu
   - Schriftart: Klappliste mit Suche. Die Familien stehen alphabetisch, zuerst die mitgelieferten. Eine fehlende Schrift steht oben mit „(fehlt)“.
   - Schrift: Fett, Kursiv (Umschalter)
   - Höhe: Längenfeld mit Einheit und Formeln wie die anderen Felder
   - Zeichenabstand: in %
   - Winkel: nur bei „Text“
   - Platzierung oberhalb/unterhalb und „An Pfad anpassen“: nur auf Pfad
   - Umkehren waagerecht und senkrecht
   - Ausrichtung waagerecht, senkrecht (senkrecht nur bei „Text“)
   - OK, Abbrechen
4. **Vorgaben (Annahme):**
   - Schrift: „Arial“, wenn installiert, sonst Montserrat
   - Höhe: 5 mm (in Zoll-Dokumenten 0,2 in)
   - Abstand 0, links, unten, nicht umgekehrt
   - Innerhalb eines Programmlaufs merkt sich das Werkzeug die zuletzt benutzten Werte, außer dem Text.
5. **Winkel:** Das Feld zeigt die Richtung der Grundlinie. Ein neuer Wert dreht den Rahmen um Ecke 0. Die Waagerecht-Abhängigkeit des neuen Rahmens fällt dabei weg. Hält eine Abhängigkeit des Nutzers den Winkel, wird mit Meldung abgelehnt. **Abweichung:** Fusion hat dafür einen Drehgriff. Der Griff ist Folgepunkt.
6. **Vorschau:** Jede Änderung zeigt sofort das Ergebnis, ohne etwas in die Skizze zu schreiben (Vorbild `pattern_preview`). Das Setzen läuft beim Tippen höchstens einmal je Bild.
7. **OK** legt den Text in einem Rückgängig-Schritt an („Text erstellen“). Ein leerer Text oder einer nur aus Leerzeichen wird abgelehnt. **Esc, Rechtsklick, Werkzeugwechsel und Abbrechen** hinterlassen nichts, auch keinen Rahmen und keinen Rückgängig-Schritt.
8. **Ändern:**
   - Doppelklick auf eine Textkurve öffnet den Abschnitt mit den Werten des Textes.
   - Der Knopf „Text bearbeiten“ im Abschnitt „Bearbeiten“ des Skizzenfelds tut dasselbe, wenn ein Text gewählt ist. Das ist eine Abweichung, weil der Rechtsklick in der Skizze abbricht, wie bei den Griffen in Skizzen 7.
   - OK ist ein Rückgängig-Schritt („Text ändern“), Abbrechen stellt alles wieder her.
   - Ein Wechsel von „Text“ zu „auf Pfad“ löscht den Rahmen und die Abhängigkeiten daran. Ein Wechsel von „auf Pfad“ zu „Text“ legt einen Rahmen an, der den Text an seiner Stelle umfasst, mit Winkel 0.
   - Belegbare Aktion `skizze.text_bearbeiten`.
9. **Text auflösen** (en „Explode Text“): im Ändern-Menü, im Abschnitt „Bearbeiten“ des Skizzenfelds und als Aktion `skizze.text_aufloesen`. Er wirkt auf alle gewählten Texte, als ein Rückgängig-Schritt.
10. **Hinweise:** Bei Ersatz der Schrift steht im Abschnitt „Text“ eine Zeile: „Schrift „X“ fehlt auf diesem Rechner, ersetzt durch Montserrat.“ Im Verlauf bekommt die Skizze eine Warnung wie bei verlorener Projektion (`feature-sketch-font-missing`). Nachgebildetes Fett oder Kursiv und fehlende Zeichen bekommen je eine Zeile im Abschnitt.
11. **Symbole:** eigene `HandIcon` für Text und Text auflösen, in `HAND_DRAWN`, ohne Autodesk-Material.
12. **Texte** nur über `tr!`/`msg!`, de und en. `docs/funktionen.md` nachführen.

### D. Profile und Extrudieren

1. Textkurven gehen in die Profilsuche wie alle Kurven. Innenkonturen (o, a, e, B …) werden Löcher in `ProfileRef`. Überlappende Konturen (Schreibschriften, manche variablen Schriften) zerfallen in Teilflächen, wie in Fusion. **Abweichung:** Fusion meldet hier oft Fehler, hier entstehen einfach mehrere Profile.
2. **Ganzer Text als Profil** (entspricht Fusions „Text als Profil wählen“, **Annahme** zur Bedienung). Das heutige Umschalten je Klick in `profile_pick.rs` bleibt, es wird nur auf Gruppen erweitert:
   - Ein **Textprofil** ist eines, dessen Kanten alle aus Kurven desselben Textes stammen, auch Teilflächen überlappender Glyphen.
   - Ein einfacher Klick auf ein Textprofil schaltet **alle** Profile dieses Textes gemeinsam um. Sind alle gewählt, werden alle abgewählt, sonst alle gewählt. Ein Klick auf einen zweiten Text nimmt ihn dazu.
   - **Strg+Klick** schaltet ein einzelnes Profil um, auch innerhalb eines Textes. An anderen Profilen wirkt Strg+Klick wie ein einfacher Klick. Umschalt bleibt ohne neue Bedeutung.
   - In der Liste mit Häkchen steht je Text eine Zeile („Text „Hallo““), die alle seine Profile umschaltet. Ist nur ein Teil gewählt, zeigt das Häkchen den Zwischenzustand. Andere Profile bleiben eine Zeile je Profil.
   - Das gilt in allen Dialogen, die Profile wählen (Extrudieren, Drehen, Bearbeiten).
3. **Messtest im Release:** 40 Zeichen Montserrat (Höhe 5) ergeben ihre Profile in höchstens 150 ms (Median). Ein Text mit 200 Zeichen in höchstens 1 s. Reicht das O(n²)-Schneiden nicht, wird es für Textkurven beschleunigt. Kurven verschiedener Glyphen, deren Hüllquader sich nicht schneiden, werden dabei nie verglichen, über ein Raster wie im Fang-Index. **Annahme** zu den Zahlen. Ist das Ziel nicht erreichbar, wird mit gemessenen Zahlen gemeldet und nicht still gesenkt.
4. Das Neusetzen beim Lösen (`refresh_texts`) kostet bei 200 Zeichen höchstens 20 ms je Bild, wenn sich nur der Rahmen bewegt.

### E. Was am Text hängt

| Befehl | Text im Rahmen | Text auf Pfad |
|---|---|---|
| Maße, Abhängigkeiten | an Rahmenlinien und Ecken wie an Linien und Punkten. An Textkurven abgelehnt: „Maße und Abhängigkeiten gehen an den Rahmen oder den Pfad, nicht an die Buchstaben.“ | am Pfad wie gewohnt. An Textkurven abgelehnt (dieselbe Meldung) |
| Verschieben, Kopieren, Drehen | bewegt die Rahmenpunkte, der Text folgt. Kopieren legt einen zweiten Text mit gleichen Werten an. Sind die Ecken gehalten, gilt die vorhandene Meldung `sketch-move-fixed` | abgelehnt, wenn der Pfad nicht mit gewählt ist: „Text auf Pfad folgt seinem Pfad.“ Mit Pfad bewegt sich der Pfad, der Text folgt. Kopieren mit Pfad kopiert beides |
| Skalieren | Rahmenpunkte skaliert, Höhe mit demselben Faktor | wie Verschieben, Höhe mit dem Faktor |
| Spiegeln | abgelehnt: „Text lässt sich nicht spiegeln. Nutze Umkehren oder löse ihn auf.“ **Annahme** | ebenso |
| Muster rechteckig/kreisförmig | abgelehnt: „Text lässt sich nicht als Muster vervielfältigen. Löse ihn zuerst auf.“ **Annahme** | ebenso |
| Versatz | abgelehnt: „Versatz geht nicht am Text. Löse ihn zuerst auf.“ (Fusion: Forum) | ebenso |
| Stutzen, Dehnen, Brechen, Abrunden, Fase | an Textkurven abgelehnt: „Text lässt sich nicht stutzen. Löse ihn zuerst auf.“ (je Befehl eigener Text). Andere Kurven werden **an** Textkurven ganz normal gestutzt, Textkurven zählen dabei als Schnittkanten | ebenso |
| Projizieren, Schneiden | nicht betroffen | nicht betroffen |
| Profile, Extrudieren | ja, D | ja, D |
| Konstruktion umschalten | ganzer Text (B10) | ganzer Text |
| Löschen | nur als Ganzes (B8) | nur als Ganzes. Der Pfad lässt sich nicht löschen, solange Text daran hängt |
| Skizze mit Text als Musterquelle oder Spiegelquelle, wenn nur Rahmenlinien gewählt sind | die Rahmenlinien werden wie Linien kopiert, es entsteht kein Text. **Annahme** | – |

Jede Ablehnung hinterlässt nichts und keinen Rückgängig-Schritt. Die vollständige Liste der Stellen liefert der Compiler über die erschöpfenden `match`, plus eine Prüfung „ist Textkurve“ in allen Befehlen.

### F. Text auflösen

1. Die erzeugten Kurven bleiben mit ihren Ids und Punkten, werden aber normale freie Kurven: Linien, Kegelschnitte, Kontrollpunkt-Splines. So bleiben Extrusionen an ihren Profilen. Wie in Fusion bekommt jeder Punkt `Fixed` an seiner aktuellen Stelle, auch Scheitel und Kontrollpunkte. Das lässt sich einzeln lösen.
2. Rahmen, Ecken und die Abhängigkeiten am Rahmen werden gelöscht. Ein Pfad bleibt.
3. Das `TextData` wird gelöscht. Text und Schrift gehen verloren, wie in Fusion. Zurück geht es nur über Rückgängig.
4. Ein Rückgängig-Schritt („Text auflösen“), auch bei mehreren Texten.

### G. Speichern und Laden

1. **Format 18** nur, wenn eine Skizze ein `TextData` enthält, auch ein gelöschtes, wie `curve_entries`. Alle anderen Dateien bleiben bytegleich und behalten ihr Format.
2. `SketchDto` bekommt `texte` (mit `skip_serializing_if = Vec::is_empty`). Die Felder heißen deutsch wie die übrigen: `inhalt`, `schrift`, `fett`, `kursiv`, `hoehe`, `abstand`, `umkehren_waagerecht`, `umkehren_senkrecht`, `lage` (`rahmen` mit `linien`, `ecken`, `waagerecht`, `senkrecht` oder `pfad` mit `kurve`, `oberhalb`, `anpassen`, `waagerecht`), `kurven`, `punkte`, `lebt`.
3. Die erzeugten Kurven und Punkte stehen als normale Einträge in der Datei, mit der Lage beim Speichern.
4. **Beim Laden** wird jeder Text mit der Schrift auf diesem Rechner neu gesetzt (`refresh_texts`), mit gleichen Ids nach B3.
   - **Zeitpunkt:** Die Schriftliste wird beim Programmstart im Hintergrund aufgebaut. Ist sie beim Laden noch nicht fertig, gilt der Zustand „noch nicht gesetzt“: Die gespeicherten Kurven bleiben unverändert stehen und rechnen mit, es gibt noch keinen Hinweis und keine Warnung. Sobald die Liste da ist, wird **einmal** neu gesetzt. Erst dann entstehen Hinweis und Warnung. Kein Neusetzen mit der Ersatzschrift vorab, damit Kurven und Ids nicht zweimal wechseln.
   - Neusetzen nach dem Laden ist kein Rückgängig-Schritt und macht das Dokument nicht „geändert“. Liefert es dieselben Kurven, ändert sich nichts. Weicht die Form ab (fehlende oder andere Schrift), gilt das Dokument als geändert, weil sonst Speichern etwas anderes schriebe, als zu sehen ist.
   - Der Text-Abschnitt und „Text auflösen“ warten, bis die Liste fertig ist, und zeigen bis dahin „Schriften werden gelesen …“. Fehlt die Schrift: Montserrat, Hinweis und Warnung nach C10. Die Form kann sich ändern, das ist Dennis' Entscheidung. Der gespeicherte Name bleibt erhalten. Wer die Datei ohne Änderung speichert, behält ihn.
5. `from_data` prüft streng: Zahlen endlich, Höhe > 0, Abstand im Bereich, Rahmen mit vier Linien und Ecken, die wirklich ein Viereck bilden, Pfad vorhanden und keine Textkurve, Kurven und Punkte vorhanden und keinem zweiten Text zugeordnet, Inhalt höchstens 500 Zeichen. Eine kaputte Skizze wird mit Meldung abgelehnt und nicht still repariert.
6. Öffnet eine ältere Programmversion eine Datei mit Format 18, gilt die vorhandene Meldung zu neueren Formaten.
7. **Laden ohne Schriften (Kopf ohne Fenster, Tests):** Feature-Berechnung und Tests bekommen die Schriftquelle von außen. Ohne Quelle wird mit den mitgelieferten Schriften gesetzt. Kein Test hängt an installierten Systemschriften.

## Nicht-Ziele

Laut Auftrag: DXF/SVG einfügen, Bild als Vorlage (Canvas), 3D-Skizze, auf Fläche projizieren, Schnittkurve, Anzeige in der Skizze, Gravieren/Prägen auf gekrümmte Flächen, Release, CI, Push.

Dazu:
- einlinige SHX-Schriften (Fusion listet sie zuerst)
- Parameter im Text (`{d1}`), Text- und Höhenparameter in der Parametertabelle
- Zeilenabstand einstellbar
- automatischer Umbruch
- Drehgriff am Rahmen
- senkrechte Schreibrichtung
- Unterstreichen
- Farbschriften und Bitmap-Schriften. Glyphen ohne Umriss bleiben leer, mit Hinweis „Zeichen fehlen“.
- Text in 2D-Zeichnungen als Beschriftung

## Anti-Ziele

- Eine alte Datei ändert ihre Bytes oder ihr Format.
- Eine fehlende Schrift lässt das Programm abstürzen, hängen oder den Text verschwinden.
- Eine Schriftdatei oder Teile davon landen in der Projektdatei.
- Ändern von Lage, Höhe oder Winkel lässt eine Extrusion ihr Profil verlieren.
- Ein abgebrochener oder abgelehnter Befehl hinterlässt Rahmen, Kurven, Punkte oder einen Rückgängig-Schritt.
- Ein Befehl verändert einzelne Textkurven, sodass Text und Kurven nicht mehr zusammenpassen.
- Die Leiste wird breiter oder rollt bei 1920×1080.
- Tests hängen an den Schriften des Rechners.
- Das Durchsuchen der Schriftordner blockiert den Programmstart.

## Wie (Entscheidungen)

- **Schichten:** `planora-text` (Schriften, Setzen, Umrisse; hängt an `skrifa`, `harfrust`, `planora-math`) → `planora-sketch` (Text-Objekt, Kurven, Löser, Befehle; **keine** Schrift-Abhängigkeit, bekommt `TextShape` über eine schmale Schnittstelle) → `planora-feature`/`planora-project` (Laden, Neusetzen über eine Schriftquelle) → `planora-app` (Bedienung). Die Montserrat- und Roboto-Dateien bekommt `planora-text` ohne Kopie, zum Beispiel über einen gemeinsamen Pfad. Wie genau, entscheidet der Bauende. Die README mit den Prüfsummen bleibt gültig.
- Keine neue Fremd-Abhängigkeit, kein neuer Löser-Sonderweg: Textpunkte sind Konstanten wie Musterkopien.
- Testgetrieben:
  - Umrisse gegen bekannte Glyphen der mitgelieferten Schriften. „O“ ergibt zwei Konturen, „i“ zwei, „B“ drei. Die Anzahl der Stücke und die Lage der Hüllquader werden geprüft.
  - Exaktheit: Kurve aus Kegelschnitt bzw. Spline gegen den `skrifa`-Umriss dicht abgetastet, Abstand unter `1e-9` × Höhe.
  - Profile mit Löchern, gleiche Ids beim Neusetzen, Bytegleichheit alter Dateien, Laden mit fehlender Schrift.
  - Jede Ablehnung aus E einzeln, jeder Abbruchweg einzeln.
  - App-Tests bei 1920×1080.
- **Reihenfolge:**
  1. `planora-text`: Liste, Schnitte, Setzen, Umrisse
  2. Text-Objekt im Rahmen mit Kurven, Neusetzen, Löschen, Auswahl
  3. Format 18
  4. Text auf Pfad
  5. Abhängige Befehle aus E und Auflösen
  6. Profile, Ganzer-Text-Wahl, Messtests
  7. Oberfläche, Hinweise, Tasten, Texte, Symbole, Doku
  8. Prüfung Windows und WSL, dann Frage nach dem Handtest

## Dem Bauenden überlassen

- Dateiaufteilung, etwa `text.rs`, `text_path.rs`, `text_refresh.rs`
- Namen der Laufzeittypen
- Genaue Form der Schnittstelle zwischen `planora-text` und `planora-sketch`
- Wie die Schriftliste in den Hintergrund geht, ohne den Start zu blockieren
- Abtastdichte und Toleranzen
- Aussehen der Klappliste
- Wie das Zusammenspiel der Vorschau mit dem Löser in jedem Bild läuft

Wo beim Bauen eine Fusion-Frage auftaucht: Annahme nach bestem Wissen treffen und in `offene-punkte.md` notieren, nicht anhalten.

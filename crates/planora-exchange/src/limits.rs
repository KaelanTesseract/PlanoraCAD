//! Grenzen beim Einlesen (Spec E8, §5). Kein Speicher wird nach Zahlen aus dem Dateikopf
//! reserviert, nur nach tatsächlich Gelesenem.

/// Höchstzahl der Dreiecke eines Netzes (Spec E8).
pub const MAX_TRIANGLES: usize = 50_000_000;
/// Größte lesbare Datei: 8 GiB (Spec E8).
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Höchstzahl der Punkte: drei je Dreieck. Mehr kann ein Netz mit [`MAX_TRIANGLES`] Dreiecken
/// nicht benutzen; die Grenze hält OBJ-Dateien mit sehr vielen Punktzeilen im Rahmen.
pub const MAX_VERTICES: usize = 3 * MAX_TRIANGLES;
/// Längste Zeile einer Textdatei (1 MiB). Eine Datei ohne Zeilenumbruch wird so nicht ganz in
/// den Speicher gelesen.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Für so viele Dreiecke wird beim Lesen höchstens im Voraus Speicher reserviert; danach wächst
/// er mit dem Gelesenen.
pub const RESERVE_STEP: usize = 65_536;
/// Alle so viele Zeilen oder Dreiecke prüfen die Leser, ob abgebrochen werden soll.
pub const CHECK_EVERY: u64 = 4096;
/// Puffergröße beim Lesen von Dateien.
pub const READ_BUFFER: usize = 64 * 1024;

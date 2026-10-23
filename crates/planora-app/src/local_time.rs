//! Zeitangaben in Ortszeit: Gespeichert wird UTC („JJJJ-MM-TTThh:mm:ssZ“, siehe
//! `planora_project::fsutil::now_iso`), gezeigt wird die Ortszeit des Rechners zum jeweiligen
//! Zeitpunkt (mit der Sommer- oder Winterzeit dieses Datums) als „23.10.2026, 18:42“.
//!
//! Die Zeitzone des Rechners kommt aus einer kleinen Bibliothek; nur [`local_offset_at`]
//! berührt sie. Alles andere ist reine Rechnung.

const DAY: i64 = 86_400;

/// Ortszeit-Text zum angegebenen Zeitpunkt. Lässt sich die Zeitzone nicht ermitteln, steht
/// UTC mit dem Zusatz „ UTC“ da; ein unlesbarer Text kommt unverändert zurück.
pub fn local_time_text(iso_utc: &str) -> String {
    local_time_text_with(iso_utc, local_offset_at)
}

/// Wie [`local_time_text`], mit eigener Zeitzonenregel `offset_at` (Sekunden seit 1970 →
/// Abstand zu UTC in Sekunden). Die Regel wird mit dem angegebenen Zeitpunkt gefragt.
pub fn local_time_text_with(iso_utc: &str, offset_at: impl Fn(i64) -> Option<i32>) -> String {
    let Some(t) = parse_utc(iso_utc) else {
        return iso_utc.to_string();
    };
    match offset_at(t) {
        Some(offset) => text_at(t + i64::from(offset)),
        None => format!("{} UTC", text_at(t)),
    }
}

/// „dd.mm.yyyy, hh:mm“ für den UTC-Text plus festen Abstand in Sekunden (Sekunden werden
/// abgeschnitten). Unlesbarer Text → `None`.
pub fn format_offset(iso_utc: &str, offset_seconds: i32) -> Option<String> {
    parse_utc(iso_utc).map(|t| text_at(t + i64::from(offset_seconds)))
}

/// Abstand der Ortszeit des Rechners zu UTC in Sekunden zum angegebenen Zeitpunkt
/// (Sekunden seit 1970), samt der damals geltenden Sommerzeit. `None`, wenn unbekannt.
pub fn local_offset_at(unix_seconds: i64) -> Option<i32> {
    use chrono::{Local, Offset, TimeZone};
    Local
        .timestamp_opt(unix_seconds, 0)
        .single()
        .map(|dt| dt.offset().fix().local_minus_utc())
}

/// Sekunden seit 1970 aus genau „JJJJ-MM-TTThh:mm:ssZ“ (mit gültigem Datum und gültiger Uhrzeit).
fn parse_utc(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = &b[from..to];
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(part.iter().fold(0, |n, d| n * 10 + i64::from(d - b'0')))
    };
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) || h > 23 || mi > 59 || sec > 59 {
        return None;
    }
    Some(days_from_civil(y, m, d) * DAY + h * 3600 + mi * 60 + sec)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Tage seit 1970-01-01 (Algorithmus von Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// „dd.mm.yyyy, hh:mm“ für Sekunden seit 1970 (Gegenstück zu [`days_from_civil`]).
fn text_at(t: i64) -> String {
    let days = t.div_euclid(DAY);
    let rem = t.rem_euclid(DAY);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{d:02}.{m:02}.{y:04}, {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

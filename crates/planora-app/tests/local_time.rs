//! Ortszeit statt UTC (Aufgabe 0): reine Umrechnung mit festen Abständen, die Schicht über
//! der Zeitzone des Rechners und der Rückfall auf UTC.

use planora_app::local_time::{
    format_offset, local_offset_at, local_time_text, local_time_text_with,
};

const H: i32 = 3600;

/// Sekunden seit 1970 für einen UTC-Zeitpunkt (unabhängig vom Prüfling gerechnet:
/// Tage über eine einfache Jahres-/Monatsschleife).
fn unix(y: i64, mo: i64, d: i64, h: i64, mi: i64) -> i64 {
    let leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mut days = 0;
    for yy in 1970..y {
        days += if leap(yy) { 366 } else { 365 };
    }
    let len = [
        31,
        if leap(y) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    days += len[..(mo - 1) as usize].iter().sum::<i64>();
    days += d - 1;
    days * 86_400 + h * 3600 + mi * 60
}

fn fo(iso: &str, off: i32) -> Option<String> {
    format_offset(iso, off)
}

#[test]
fn the_normal_case_adds_the_offset() {
    assert_eq!(
        fo("2026-10-23T16:42:07Z", 2 * H).as_deref(),
        Some("23.10.2026, 18:42")
    );
    assert_eq!(
        fo("2026-10-23T16:42:07Z", 0).as_deref(),
        Some("23.10.2026, 16:42")
    );
    assert_eq!(
        fo("2026-10-23T16:42:07Z", -5 * H).as_deref(),
        Some("23.10.2026, 11:42")
    );
    // halbe und Dreiviertelstunden (Indien, Nepal)
    assert_eq!(
        fo("2026-10-23T16:42:07Z", 19_800).as_deref(),
        Some("23.10.2026, 22:12")
    );
    assert_eq!(
        fo("2026-10-23T16:00:00Z", 20_700).as_deref(),
        Some("23.10.2026, 21:45")
    );
}

#[test]
fn hours_are_24_hour_and_padded() {
    assert_eq!(
        fo("2026-10-23T13:05:00Z", 0).as_deref(),
        Some("23.10.2026, 13:05")
    );
    assert_eq!(
        fo("2026-01-02T03:04:05Z", 0).as_deref(),
        Some("02.01.2026, 03:04")
    );
    assert_eq!(
        fo("2026-10-23T12:00:00Z", 0).as_deref(),
        Some("23.10.2026, 12:00")
    );
    assert_eq!(
        fo("2026-10-23T00:00:00Z", 0).as_deref(),
        Some("23.10.2026, 00:00")
    );
}

#[test]
fn seconds_are_ignored_not_rounded() {
    assert_eq!(
        fo("2026-10-23T16:42:59Z", 0).as_deref(),
        Some("23.10.2026, 16:42")
    );
    assert_eq!(
        fo("2026-10-23T16:42:00Z", 0).as_deref(),
        Some("23.10.2026, 16:42")
    );
}

#[test]
fn the_day_rolls_over_forwards_and_backwards() {
    assert_eq!(
        fo("2026-10-23T23:30:00Z", H).as_deref(),
        Some("24.10.2026, 00:30")
    );
    assert_eq!(
        fo("2026-10-23T00:30:00Z", -H).as_deref(),
        Some("22.10.2026, 23:30")
    );
    assert_eq!(
        fo("2026-10-31T23:30:00Z", H).as_deref(),
        Some("01.11.2026, 00:30")
    );
    assert_eq!(
        fo("2026-11-01T00:30:00Z", -H).as_deref(),
        Some("31.10.2026, 23:30")
    );
    assert_eq!(
        fo("2026-04-30T22:00:00Z", 2 * H).as_deref(),
        Some("01.05.2026, 00:00")
    );
}

#[test]
fn the_year_rolls_over() {
    assert_eq!(
        fo("2026-12-31T23:30:00Z", H).as_deref(),
        Some("01.01.2027, 00:30")
    );
    assert_eq!(
        fo("2027-01-01T00:15:00Z", -H).as_deref(),
        Some("31.12.2026, 23:15")
    );
}

#[test]
fn leap_years_follow_the_calendar() {
    assert_eq!(
        fo("2028-02-28T23:30:00Z", H).as_deref(),
        Some("29.02.2028, 00:30")
    );
    assert_eq!(
        fo("2028-02-29T23:30:00Z", H).as_deref(),
        Some("01.03.2028, 00:30")
    );
    assert_eq!(
        fo("2028-03-01T00:30:00Z", -H).as_deref(),
        Some("29.02.2028, 23:30")
    );
    assert_eq!(
        fo("2026-02-28T23:30:00Z", H).as_deref(),
        Some("01.03.2026, 00:30")
    );
    assert_eq!(
        fo("2100-02-28T23:30:00Z", H).as_deref(),
        Some("01.03.2100, 00:30")
    );
    assert_eq!(
        fo("2000-02-28T23:30:00Z", H).as_deref(),
        Some("29.02.2000, 00:30")
    );
    assert_eq!(
        fo("2028-12-31T12:00:00Z", 0).as_deref(),
        Some("31.12.2028, 12:00")
    );
}

#[test]
fn invalid_texts_give_none() {
    for bad in [
        "",
        "2026-10-23T16:42:07",
        "2026-13-23T16:42:07Z",
        "2026-00-23T16:42:07Z",
        "2026-10-00T16:42:07Z",
        "2026-02-29T16:42:07Z",
        "2026-04-31T16:42:07Z",
        "2026-10-23T24:00:00Z",
        "2026-10-23T16:60:00Z",
        "2026-10-23T16:42:60Z",
        "2026-10-23 16:42:07",
        "2026-10-23 16:42:07Z",
        "2026-10-23T16:42:07Zx",
        "2026-10-23T16:42:07+",
        "2100-02-29T12:00:00Z",
        "1900-02-29T12:00:00Z",
        "2026-1x-23T16:42:07Z",
        "+026-10-23T16:42:07Z",
        "kaputt",
    ] {
        assert_eq!(fo(bad, 0), None, "„{bad}“");
        assert_eq!(fo(bad, 2 * H), None, "„{bad}“");
    }
}

#[test]
fn local_time_text_keeps_invalid_text_unchanged() {
    for bad in ["", "kaputt", "2026-10-23 16:42:07", "2026-13-23T16:42:07Z"] {
        assert_eq!(local_time_text(bad), bad);
        assert_eq!(local_time_text_with(bad, |_| Some(H)), bad);
        assert_eq!(local_time_text_with(bad, |_| None), bad);
    }
}

#[test]
fn without_a_zone_the_text_is_utc_with_a_mark() {
    assert_eq!(
        local_time_text_with("2026-10-23T16:42:07Z", |_| None),
        "23.10.2026, 16:42 UTC"
    );
    assert_eq!(
        local_time_text_with("2026-10-23T16:42:07Z", |_| Some(0)),
        "23.10.2026, 16:42"
    );
}

/// Eine nachgebaute Berliner Regel für 2026 (nur zum Prüfen der Zusammensetzung).
fn berlin_2026(t: i64) -> Option<i32> {
    let summer = t >= unix(2026, 3, 29, 1, 0) && t < unix(2026, 10, 25, 1, 0);
    Some(if summer { 2 * H } else { H })
}

#[test]
fn the_offset_is_taken_at_the_given_moment_not_today() {
    let seen = std::cell::Cell::new(0);
    let _ = local_time_text_with("2026-10-23T16:42:07Z", |t| {
        seen.set(t);
        Some(0)
    });
    assert_eq!(seen.get(), unix(2026, 10, 23, 16, 42) + 7);
    let w = |s| local_time_text_with(s, berlin_2026);
    assert_eq!(w("2026-01-15T12:00:00Z"), "15.01.2026, 13:00");
    assert_eq!(w("2026-07-15T12:00:00Z"), "15.07.2026, 14:00");
    assert_eq!(w("2026-03-29T00:00:00Z"), "29.03.2026, 01:00");
    assert_eq!(w("2026-03-29T02:00:00Z"), "29.03.2026, 04:00");
    assert_eq!(w("2026-10-25T00:00:00Z"), "25.10.2026, 02:00");
    assert_eq!(w("2026-10-25T02:00:00Z"), "25.10.2026, 03:00");
}

#[test]
fn the_machine_zone_gives_a_plausible_offset() {
    for t in [
        unix(2026, 1, 15, 12, 0),
        unix(2026, 7, 15, 12, 0),
        unix(2000, 1, 1, 0, 0),
        unix(2015, 7, 1, 12, 0),
    ] {
        let o = local_offset_at(t).expect("Zone des Rechners");
        assert!(o.abs() <= 14 * H, "{o}");
        assert_eq!(o % 900, 0, "{o}");
    }
    let iso = "2026-07-15T12:00:00Z";
    let o = local_offset_at(unix(2026, 7, 15, 12, 0)).unwrap();
    assert_eq!(Some(local_time_text(iso)), format_offset(iso, o));
}

/// Gegenprobe gegen die Zeitzone des Betriebssystems (.NET `TimeZoneInfo.Local`): Abstand
/// je Zeitpunkt, und bei Berliner Zone die festen Werte samt Umstellungstagen.
#[cfg(windows)]
#[test]
fn the_offset_matches_the_operating_system_at_each_moment() {
    let moments = [
        unix(2026, 1, 15, 12, 0),
        unix(2026, 7, 15, 12, 0),
        unix(2026, 3, 29, 0, 0),
        unix(2026, 3, 29, 2, 0),
        unix(2026, 10, 25, 0, 0),
        unix(2026, 10, 25, 2, 0),
        unix(2015, 7, 1, 12, 0),
        unix(2010, 12, 1, 12, 0),
    ];
    let list = moments
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "$z=[TimeZoneInfo]::Local; $z.Id; foreach($t in @({list})){{ [int]$z.GetUtcOffset([DateTimeOffset]::FromUnixTimeSeconds($t)).TotalSeconds }}"
    );
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("powershell");
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let zone = lines.next().expect("Zonenname").to_string();
    let os: Vec<i32> = lines.map(|l| l.parse().unwrap()).collect();
    assert_eq!(os.len(), moments.len(), "{text}");
    let ours: Vec<i32> = moments
        .iter()
        .map(|&t| local_offset_at(t).unwrap())
        .collect();
    assert_eq!(ours, os, "Zone {zone}");
    if zone == "W. Europe Standard Time" {
        assert_eq!(ours, [H, 2 * H, H, 2 * H, 2 * H, H, 2 * H, H]);
    }
}

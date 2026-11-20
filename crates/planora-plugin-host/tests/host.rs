//! Der `PluginHost` (Plug-ins Etappe 1, Task 4): Start im Hintergrund, Befehle, Ausfälle, Neustart,
//! Rechte-Nachfrage und Netz über das Ereignis-Postfach. Die Test-Pakete werden in eigene
//! Temp-Fundorte installiert.

use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::Package;
use planora_plugin_host::places::{self, Places};
use planora_plugin_host::rights::{Answer, Grant};
use planora_plugin_host::runtime::{HttpResponse, RequestResult};
use planora_plugin_host::{EntryState, HostEvent, PluginHost, PluginState};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const HALLO: &str = "planora.beispiel-hallo";
const FAELLE: &str = "planora.test-faelle";

fn orte(name: &str, pakete: &[&str]) -> Places {
    let root =
        std::env::temp_dir().join(format!("planora-plugin-host-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let p = Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    };
    for paket in pakete {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(format!("{paket}.pcplugin"));
        let pkg = Package::read(&std::fs::read(file).unwrap()).unwrap();
        places::install(&p, &pkg, None).unwrap();
    }
    p
}

fn aufraeumen(p: &Places) {
    let _ = std::fs::remove_dir_all(p.user.parent().unwrap());
}

fn state(h: &PluginHost, id: &str) -> EntryState {
    h.entries()
        .iter()
        .find(|e| e.id == id)
        .unwrap()
        .state
        .clone()
}

fn failures(h: &PluginHost, id: &str) -> u8 {
    h.entries().iter().find(|e| e.id == id).unwrap().failures
}

/// Ruft `poll` auf, bis `id` nicht mehr lädt (höchstens 60 s); gibt alle Ereignisse zurück.
fn warten(h: &mut PluginHost, id: &str) -> Vec<HostEvent> {
    let start = Instant::now();
    let mut events = Vec::new();
    loop {
        events.extend(h.poll());
        if state(h, id) != EntryState::Loading {
            return events;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "lädt zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn gestartet(name: &str, pakete: &[&str]) -> (PluginHost, Places) {
    let p = orte(name, pakete);
    let mut h = PluginHost::new(p.clone());
    h.start_all(&BTreeMap::new());
    for id in pakete {
        let id = format!("planora.{id}");
        warten(&mut h, &id);
        assert_eq!(state(&h, &id), EntryState::Running);
    }
    (h, p)
}

fn ausfaelle(events: &[HostEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, HostEvent::Failed { .. }))
        .count()
}

#[test]
fn start_registers_commands() {
    let (h, p) = gestartet("befehle", &["beispiel-hallo"]);
    let cmds = h.commands();
    let hallo = cmds
        .iter()
        .find(|c| c.full_id == "planora.beispiel-hallo.hallo")
        .expect("Befehl fehlt");
    assert_eq!(hallo.plugin, HALLO);
    assert_eq!(hallo.label, "Say hello");
    aufraeumen(&p);
}

#[test]
fn disabled_not_started() {
    let p = orte("aus", &["beispiel-hallo"]);
    let mut h = PluginHost::new(p.clone());
    let config = BTreeMap::from([(
        HALLO.to_string(),
        PluginState {
            enabled: false,
            ..Default::default()
        },
    )]);
    h.start_all(&config);
    assert_eq!(state(&h, HALLO), EntryState::Off);
    std::thread::sleep(Duration::from_millis(300));
    assert!(h.poll().is_empty());
    assert_eq!(state(&h, HALLO), EntryState::Off);
    assert!(h.commands().is_empty());
    assert_eq!(h.plugin_states().get(HALLO).map(|c| c.enabled), Some(false));

    // Einschalten startet es und meldet die Änderung zum Speichern.
    h.set_enabled(HALLO, true);
    let events = warten(&mut h, HALLO);
    assert!(events.iter().any(|e| matches!(e, HostEvent::Changed)));
    assert_eq!(state(&h, HALLO), EntryState::Running);
    assert!(
        !h.plugin_states().contains_key(HALLO),
        "Voreinstellung wird nicht geschrieben"
    );
    h.set_enabled(HALLO, false);
    assert_eq!(state(&h, HALLO), EntryState::Off);
    assert!(h.commands().is_empty());
    assert!(h.poll().iter().any(|e| matches!(e, HostEvent::Changed)));
    aufraeumen(&p);
}

#[test]
fn failure_counts_and_stops() {
    let (mut h, p) = gestartet("stopp", &["test-faelle"]);
    for n in 1..=3u8 {
        h.run_command("planora.test-faelle", "panik");
        h.poll();
        assert_eq!(failures(&h, FAELLE), n);
        if n < 3 {
            assert!(matches!(state(&h, FAELLE), EntryState::Failed(_)));
            assert!(
                h.commands().is_empty(),
                "ein ausgefallenes Plug-in hat keine Befehle"
            );
            h.restart(FAELLE);
            warten(&mut h, FAELLE);
            assert_eq!(state(&h, FAELLE), EntryState::Running);
        }
    }
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);
    h.restart(FAELLE);
    std::thread::sleep(Duration::from_millis(300));
    h.poll();
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);
    h.set_enabled(FAELLE, true);
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);

    // Erst ein neuer Host (Programmneustart) startet es wieder.
    let mut h = PluginHost::new(p.clone());
    h.start_all(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(failures(&h, FAELLE), 0);
    aufraeumen(&p);
}

#[test]
fn failure_event_once_per_failure() {
    let (mut h, p) = gestartet("ereignis", &["test-faelle"]);
    h.run_command("planora.test-faelle", "panik");
    let events = h.poll();
    assert_eq!(ausfaelle(&events), 1, "{events:?}");
    let Some(HostEvent::Failed { plugin, reason }) = events
        .iter()
        .find(|e| matches!(e, HostEvent::Failed { .. }))
    else {
        unreachable!()
    };
    assert_eq!(plugin, FAELLE);
    assert!(!reason.is_empty());
    assert_eq!(ausfaelle(&h.poll()), 0);
    // Weitere Befehle an das ausgefallene Plug-in tun nichts.
    h.run_command("planora.test-faelle", "panik");
    assert_eq!(ausfaelle(&h.poll()), 0);
    assert_eq!(failures(&h, FAELLE), 1);

    // Ein Fehler, den das Plug-in selbst meldet, ist kein Ausfall, sondern eine Meldung.
    h.restart(FAELLE);
    warten(&mut h, FAELLE);
    h.run_command("planora.test-faelle", "fehler");
    let events = h.poll();
    assert_eq!(ausfaelle(&events), 0);
    assert!(events.iter().any(
        |e| matches!(e, HostEvent::Message { plugin, text, .. } if plugin == FAELLE && text.contains("absichtlich"))
    ));
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    aufraeumen(&p);
}

/// Die Nachfragen (`Ask`) in `events`: (Anfrage, Recht, Generation).
fn asks(events: &[HostEvent]) -> Vec<(u64, Right, u64)> {
    events
        .iter()
        .filter_map(|e| match e {
            HostEvent::Ask {
                plugin,
                request,
                right,
                generation,
            } if plugin == FAELLE => Some((*request, *right, *generation)),
            _ => None,
        })
        .collect()
}

/// Die Netzanfragen in `events`: (Anfrage, Adresse, Generation).
fn nets(events: &[HostEvent]) -> Vec<(u64, String, u64)> {
    events
        .iter()
        .filter_map(|e| match e {
            HostEvent::Net {
                plugin,
                request,
                url,
                generation,
            } if plugin == FAELLE => Some((*request, url.clone(), *generation)),
            _ => None,
        })
        .collect()
}

#[test]
fn restart_resets_instance() {
    let (mut h, p) = gestartet("neustart", &["test-faelle"]);
    h.run_command("planora.test-faelle", "recht");
    let first = asks(&h.poll());
    assert_eq!(first.len(), 1);
    let (_, _, g1) = first[0];

    h.run_command("planora.test-faelle", "panik");
    h.poll();
    h.restart(FAELLE);
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert!(
        h.commands()
            .iter()
            .any(|c| c.full_id == "planora.test-faelle.panik")
    );

    h.run_command("planora.test-faelle", "recht");
    let second = asks(&h.poll());
    assert_eq!(second.len(), 1, "die neue Instanz fragt wieder");
    let (req2, _, g2) = second[0];
    assert_ne!(g1, g2, "neue Generation");

    // Eine Antwort an die alte Generation wird still verworfen.
    h.answer(FAELLE, g1, req2, Answer::Allow);
    assert!(h.poll().is_empty());
    assert!(!h.plugin_states().contains_key(FAELLE));
    h.answer(FAELLE, g2, req2, Answer::Allow);
    assert!(h.poll().iter().any(|e| matches!(e, HostEvent::Changed)));
    assert_eq!(
        h.plugin_states()[FAELLE]
            .rights
            .get("net")
            .map(String::as_str),
        Some("allowed")
    );
    aufraeumen(&p);
}

#[test]
fn rights_and_net_through_mailbox() {
    let (mut h, p) = gestartet("netz", &["test-faelle"]);
    // Zwei Netzanfragen, solange gefragt wird: nur eine Nachfrage.
    h.run_command("planora.test-faelle", "netz");
    h.run_command("planora.test-faelle", "netz");
    let events = h.poll();
    let a = asks(&events);
    assert_eq!(a.len(), 1, "{events:?}");
    assert!(nets(&events).is_empty());
    let (req, right, g) = a[0];
    assert_eq!(right, Right::Net);

    // „Nur diesmal“: beide Anfragen gehen ins Netz, nichts zu speichern.
    h.answer(FAELLE, g, req, Answer::Once);
    let events = h.poll();
    let n = nets(&events);
    assert_eq!(n.len(), 2, "{events:?}");
    assert!(
        n.iter()
            .all(|(_, url, generation)| url == "https://example.org/" && *generation == g)
    );
    assert!(!events.iter().any(|e| matches!(e, HostEvent::Changed)));

    // Ergebnisse kommen beim nächsten `poll` an; eine alte Generation wird verworfen.
    h.net_result(
        FAELLE,
        g,
        n[0].0,
        RequestResult::Http(HttpResponse {
            status: 200,
            body: b"ok".to_vec(),
        }),
    );
    h.net_result(FAELLE, g + 1000, n[1].0, RequestResult::Failed("x".into()));
    assert!(h.poll().is_empty());
    assert_eq!(state(&h, FAELLE), EntryState::Running);

    // „Nur diesmal“ galt nur für diese Anfragen: die nächste fragt wieder.
    h.run_command("planora.test-faelle", "netz");
    let events = h.poll();
    let a = asks(&events);
    assert_eq!(a.len(), 1, "{events:?}");
    assert!(nets(&events).is_empty());
    h.answer(FAELLE, g, a[0].0, Answer::Allow);
    let events = h.poll();
    assert_eq!(nets(&events).len(), 1);
    assert!(events.iter().any(|e| matches!(e, HostEvent::Changed)));

    // Jetzt ist `net` dauerhaft erlaubt: direkt ins Netz, ohne Nachfrage.
    h.run_command("planora.test-faelle", "netz");
    let events = h.poll();
    assert!(asks(&events).is_empty());
    assert_eq!(nets(&events).len(), 1);

    // Zurückgenommen: es wird wieder gefragt, und die Änderung ist zu speichern.
    h.revoke(FAELLE, Right::Net);
    h.run_command("planora.test-faelle", "netz");
    let events = h.poll();
    assert!(events.iter().any(|e| matches!(e, HostEvent::Changed)));
    assert_eq!(asks(&events).len(), 1, "{events:?}");
    aufraeumen(&p);
}

#[test]
fn saved_rights_from_config() {
    let p = orte("gespeichert", &["test-faelle"]);
    let mut h = PluginHost::new(p.clone());
    let config = BTreeMap::from([(
        FAELLE.to_string(),
        PluginState {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "denied".to_string())]),
            author: Some("Dennis Guse".to_string()),
            net_hosts: vec!["example.org".to_string()],
            source: Some("file".to_string()),
        },
    )]);
    h.start_all(&config);
    warten(&mut h, FAELLE);
    // Abgelehnt: keine Nachfrage, kein Netz; das Ergebnis geht still an das Plug-in.
    h.run_command("planora.test-faelle", "netz");
    h.run_command("planora.test-faelle", "recht");
    let events = h.poll();
    assert!(asks(&events).is_empty(), "{events:?}");
    assert!(nets(&events).is_empty());
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(h.plugin_states(), config);
    aufraeumen(&p);
}

/// Ereignisse von `FAELLE`, die zur Instanz gehören (Nachfrage, Netz, Dialog).
fn instanz_ereignisse(events: &[HostEvent]) -> usize {
    events
        .iter()
        .filter(|e| match e {
            HostEvent::Ask { plugin, .. }
            | HostEvent::Net { plugin, .. }
            | HostEvent::OpenDialog { plugin, .. } => plugin == FAELLE,
            _ => false,
        })
        .count()
}

#[test]
fn events_dropped_after_failure_or_stop() {
    let (mut h, p) = gestartet("verworfen", &["test-faelle"]);
    // Dialog und Nachfrage stehen noch im Ausgang, dann fällt das Plug-in aus.
    h.run_command("planora.test-faelle", "langsam-dialog");
    h.run_command("planora.test-faelle", "recht");
    h.run_command("planora.test-faelle", "panik");
    let events = h.poll();
    assert_eq!(instanz_ereignisse(&events), 0, "{events:?}");
    assert_eq!(ausfaelle(&events), 1);

    // Dasselbe beim Neustart und beim Ausschalten.
    h.restart(FAELLE);
    warten(&mut h, FAELLE);
    h.run_command("planora.test-faelle", "recht");
    h.restart(FAELLE);
    let events = warten(&mut h, FAELLE);
    assert_eq!(instanz_ereignisse(&events), 0, "{events:?}");
    h.run_command("planora.test-faelle", "netz");
    h.set_enabled(FAELLE, false);
    let events = h.poll();
    assert_eq!(instanz_ereignisse(&events), 0, "{events:?}");
    assert!(events.iter().any(|e| matches!(e, HostEvent::Changed)));
    aufraeumen(&p);
}

#[test]
fn rescan_restarts_changed_only() {
    let (mut h, p) = gestartet("rescan", &["test-faelle", "beispiel-hallo"]);
    h.run_command("planora.test-faelle", "recht");
    let (_, _, g1) = asks(&h.poll())[0];

    // Neuere Version installiert (wie „Plug-in installieren…“ mit einem neueren Paket).
    let toml = p.user.join(FAELLE).join("plugin.toml");
    let text = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(
        &toml,
        text.replace("version = \"0.1.0\"", "version = \"0.2.0\""),
    )
    .unwrap();
    h.rescan(&BTreeMap::new());
    assert_eq!(state(&h, FAELLE), EntryState::Loading);
    assert_eq!(
        state(&h, HALLO),
        EntryState::Running,
        "unverändert läuft weiter"
    );
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    let e = h.entries().iter().find(|e| e.id == FAELLE).unwrap();
    assert_eq!(
        e.found[e.active.unwrap()].manifest.version.to_string(),
        "0.2.0"
    );
    h.run_command("planora.test-faelle", "recht");
    let (_, _, g2) = asks(&h.poll())[0];
    assert_ne!(g1, g2, "neue Generation");
    assert!(
        h.commands()
            .iter()
            .any(|c| c.full_id == "planora.beispiel-hallo.hallo")
    );

    // Entfernt: verschwindet; neu installiert und in `states` ausgeschaltet: bleibt aus.
    places::uninstall(&p, HALLO).unwrap();
    h.rescan(&BTreeMap::new());
    assert!(h.entries().iter().all(|e| e.id != HALLO));
    assert!(h.commands().iter().all(|c| c.plugin != HALLO));
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/beispiel-hallo.pcplugin");
    places::install(
        &p,
        &Package::read(&std::fs::read(file).unwrap()).unwrap(),
        None,
    )
    .unwrap();
    let aus = BTreeMap::from([(
        HALLO.to_string(),
        PluginState {
            enabled: false,
            ..Default::default()
        },
    )]);
    h.rescan(&aus);
    assert_eq!(state(&h, HALLO), EntryState::Off);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    aufraeumen(&p);
}

/// test-faelle installiert und gestartet, `net` dauerhaft erlaubt (wie aus der Konfiguration).
fn mit_netzrecht(name: &str, bundled_too: bool) -> (PluginHost, Places) {
    let p = orte(name, &["test-faelle"]);
    if bundled_too {
        let bundled = Places {
            bundled: Vec::new(),
            user: p.bundled[0].clone(),
            cache: p.cache.clone(),
        };
        let file =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/test-faelle.pcplugin");
        places::install(
            &bundled,
            &Package::read(&std::fs::read(file).unwrap()).unwrap(),
            None,
        )
        .unwrap();
    }
    let mut h = PluginHost::new(p.clone());
    h.start_all(&BTreeMap::from([(
        FAELLE.to_string(),
        PluginState {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "allowed".to_string())]),
            author: Some("Dennis Guse".to_string()),
            net_hosts: vec!["example.org".to_string()],
            source: Some("file".to_string()),
        },
    )]));
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(net(&h), Some(Grant::Allowed));
    (h, p)
}

fn net(h: &PluginHost) -> Option<Grant> {
    h.entries()
        .iter()
        .find(|e| e.id == FAELLE)
        .unwrap()
        .rights
        .state(Right::Net)
}

/// Ändert das installierte plugin.toml von test-faelle (wie ein Ersetzen durch ein neues Paket).
fn manifest_aendern(p: &Places, from: &str, to: &str) {
    let toml = p.user.join(FAELLE).join("plugin.toml");
    let text = std::fs::read_to_string(&toml).unwrap();
    assert!(text.contains(from), "{text}");
    std::fs::write(&toml, text.replace(from, to)).unwrap();
}

#[test]
fn replace_keeps_rights_of_same_author_only() {
    // Gleicher Autor, neue Version: das Recht bleibt.
    let (mut h, p) = mit_netzrecht("ersetzen-autor", false);
    manifest_aendern(&p, "version = \"0.1.0\"", "version = \"0.2.0\"");
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(net(&h), Some(Grant::Allowed), "gleicher Autor");
    // Rechner nur anders geschrieben: bleibt (ohne Rücksicht auf Groß- und Kleinschreibung).
    manifest_aendern(&p, "\"example.org\"", "\"EXAMPLE.org\"");
    manifest_aendern(&p, "version = \"0.2.0\"", "version = \"0.2.1\"");
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(net(&h), Some(Grant::Allowed), "gleiche Rechner");
    // Andere Rechner: `net` fällt weg.
    manifest_aendern(&p, "\"EXAMPLE.org\"", "\"example.com\"");
    manifest_aendern(&p, "version = \"0.2.1\"", "version = \"0.3.0\"");
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(net(&h), None, "andere Rechner");
    assert!(
        !h.plugin_states().contains_key(FAELLE),
        "nichts mehr zu speichern"
    );
    aufraeumen(&p);

    // Anderer Autor: alle Rechte fallen weg.
    let (mut h, p) = mit_netzrecht("ersetzen-fremd", false);
    manifest_aendern(&p, "author = \"Dennis Guse\"", "author = \"Jemand anders\"");
    manifest_aendern(&p, "version = \"0.1.0\"", "version = \"0.2.0\"");
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(net(&h), None, "anderer Autor");
    aufraeumen(&p);

    // Ein Recht, das die neue Version nicht mehr verlangt, fällt weg.
    let (mut h, p) = mit_netzrecht("ersetzen-ohne-netz", false);
    manifest_aendern(&p, "rights = [\"net\"]", "rights = []");
    manifest_aendern(&p, "net-hosts = [\"example.org\"]", "");
    manifest_aendern(&p, "version = \"0.1.0\"", "version = \"0.2.0\"");
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(net(&h), None, "nicht mehr verlangt");
    aufraeumen(&p);
}

#[test]
fn forget_state_leaves_bundled_copy_without_rights() {
    let (mut h, p) = mit_netzrecht("vergessen", true);
    let e = h.entries().iter().find(|e| e.id == FAELLE).unwrap();
    assert_eq!(e.found.len(), 2);
    assert_eq!(e.found[e.active.unwrap()].origin, places::Origin::User);
    places::uninstall(&p, FAELLE).unwrap();
    h.forget_state(FAELLE);
    // Der Stand der Konfiguration (noch mit dem Recht) zählt nach dem Vergessen nicht mehr.
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    let e = h.entries().iter().find(|e| e.id == FAELLE).unwrap();
    assert_eq!(e.found[e.active.unwrap()].origin, places::Origin::Bundled);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(net(&h), None);
    assert!(!h.plugin_states().contains_key(FAELLE));
    aufraeumen(&p);
}

#[test]
fn replaced_restarts_same_version() {
    let (mut h, p) = mit_netzrecht("gleiche-version", false);
    h.run_command("planora.test-faelle", "netz");
    let g1 = h.poll().iter().find_map(|e| match e {
        HostEvent::Net { generation, .. } | HostEvent::Ask { generation, .. } => Some(*generation),
        _ => None,
    });
    let old = {
        let e = h.entries().iter().find(|e| e.id == FAELLE).unwrap();
        e.found[e.active.unwrap()].clone()
    };
    // Gleiche Version erneut installiert (anderer Autor): rescan lässt es laufen, `replaced`
    // gleicht die Rechte ab und startet neu.
    manifest_aendern(&p, "author = \"Dennis Guse\"", "author = \"Jemand anders\"");
    h.rescan(&BTreeMap::new());
    assert_eq!(
        state(&h, FAELLE),
        EntryState::Running,
        "rescan allein startet nicht neu"
    );
    h.replaced(FAELLE, Some(&old));
    assert_eq!(state(&h, FAELLE), EntryState::Loading, "neu gestartet");
    assert_eq!(net(&h), None, "anderer Autor");
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    h.run_command("planora.test-faelle", "netz");
    let g2 = h.poll().iter().find_map(|e| match e {
        HostEvent::Net { generation, .. } | HostEvent::Ask { generation, .. } => Some(*generation),
        _ => None,
    });
    assert!(
        g1.is_some() && g2.is_some() && g1 != g2,
        "neue Generation: {g1:?} {g2:?}"
    );
    aufraeumen(&p);
}

/// Gespeicherte Rechte gelten beim Start nur für dasselbe Plug-in wie beim Erteilen: gleicher
/// Autor, für `net` dieselben Rechner (Groß- und Kleinschreibung egal); Rechte, die das Manifest
/// nicht verlangt, fallen weg. Einträge ohne gespeicherten Autor (frühere Stände dieses Zweigs)
/// gelten als unbekannt: neu fragen.
#[test]
fn saved_rights_need_same_author_and_hosts() {
    let faelle_von = |author: Option<&str>, hosts: &[&str], source: Option<&str>| PluginState {
        enabled: true,
        rights: BTreeMap::from([
            ("net".to_string(), "allowed".to_string()),
            ("model-read".to_string(), "allowed".to_string()),
        ]),
        author: author.map(str::to_string),
        net_hosts: hosts.iter().map(|h| h.to_string()).collect(),
        source: source.map(str::to_string),
    };
    let faelle = |author: Option<&str>, hosts: &[&str]| faelle_von(author, hosts, Some("file"));
    let cases = [
        (
            "gleich",
            faelle(Some("Dennis Guse"), &["example.org"]),
            Some(Grant::Allowed),
        ),
        (
            "gross",
            faelle(Some("Dennis Guse"), &["EXAMPLE.org"]),
            Some(Grant::Allowed),
        ),
        ("unbekannt", faelle(None, &["example.org"]), None),
        (
            "fremd",
            faelle(Some("Jemand anderes"), &["example.org"]),
            None,
        ),
        (
            "rechner",
            faelle(Some("Dennis Guse"), &["example.com"]),
            None,
        ),
        ("ohne-rechner", faelle(Some("Dennis Guse"), &[]), None),
        // Herkunft: unbekannt (früherer Stand) oder eine andere als die Datei: neu fragen.
        (
            "herkunft-fehlt",
            faelle_von(Some("Dennis Guse"), &["example.org"], None),
            None,
        ),
        (
            "herkunft-link",
            faelle_von(
                Some("Dennis Guse"),
                &["example.org"],
                Some("https://github.com/a/b"),
            ),
            None,
        ),
        (
            "herkunft-mitgeliefert",
            faelle_von(Some("Dennis Guse"), &["example.org"], Some("bundled")),
            None,
        ),
    ];
    for (name, saved, expected) in cases {
        let p = orte(&format!("autor-{name}"), &["test-faelle"]);
        let mut h = PluginHost::new(p.clone());
        h.start_all(&BTreeMap::from([(FAELLE.to_string(), saved)]));
        warten(&mut h, FAELLE);
        assert_eq!(net(&h), expected, "{name}");
        let e = h.entries().iter().find(|e| e.id == FAELLE).unwrap();
        assert_eq!(
            e.rights.state(Right::ModelRead),
            None,
            "{name}: nicht verlangt"
        );
        // Gespeichert wird mit Autor und Rechnern des laufenden Manifests.
        let states = h.plugin_states();
        match expected {
            Some(_) => {
                let s = &states[FAELLE];
                assert_eq!(s.author.as_deref(), Some("Dennis Guse"), "{name}");
                assert_eq!(s.net_hosts, ["example.org"], "{name}");
                assert_eq!(s.source.as_deref(), Some("file"), "{name}");
                assert_eq!(
                    s.rights,
                    BTreeMap::from([("net".to_string(), "allowed".to_string())])
                );
            }
            None => assert!(!states.contains_key(FAELLE), "{name}: {states:?}"),
        }
        aufraeumen(&p);
    }
}

/// Messung für Task 9 (Start der Plug-ins ohne und mit Zwischenspeicher):
/// `cargo test --release -p planora-plugin-host --test host messung_start -- --ignored --nocapture`.
/// Kein Test im üblichen Sinn: sie schreibt nur Zeiten nach stderr.
#[test]
#[ignore = "Messung, siehe docs/plugins/entwickler.md"]
fn messung_start() {
    let leer = orte("messung-leer", &[]);
    let t = Instant::now();
    let mut h = PluginHost::new(leer.clone());
    h.start_all(&BTreeMap::new());
    let ohne = t.elapsed();
    drop(h);
    aufraeumen(&leer);

    let p = orte("messung", &["beispiel-hallo", "test-faelle"]);
    let mut zeiten = Vec::new();
    for _ in 0..2 {
        let t = Instant::now();
        let mut h = PluginHost::new(p.clone());
        h.start_all(&BTreeMap::new());
        for id in [HALLO, FAELLE] {
            warten(&mut h, id);
            assert_eq!(state(&h, id), EntryState::Running);
        }
        zeiten.push(t.elapsed());
        drop(h);
    }
    aufraeumen(&p);
    eprintln!("MESSUNG ohne Plug-ins: {ohne:?}");
    eprintln!("MESSUNG erster Start (Übersetzen): {:?}", zeiten[0]);
    eprintln!("MESSUNG zweiter Start (Zwischenspeicher): {:?}", zeiten[1]);
}

/// Ein Plug-in, das in jedem `on-result` ~150 ms rechnet und neue Arbeit anfordert, hält die
/// Oberfläche je Bild höchstens ~200 ms plus eine Frist auf und wird nach drei solchen Bildern
/// angehalten (ein Ausfall „zu viel Arbeit“).
#[test]
fn endless_work_in_on_result_is_stopped() {
    let (mut h, p) = gestartet("dauerlast", &["test-faelle"]);
    h.run_command(FAELLE, "dauerlast");
    let mut events = Vec::new();
    let mut bilder = 0;
    while matches!(state(&h, FAELLE), EntryState::Running) {
        bilder += 1;
        assert!(bilder <= 10, "nicht angehalten");
        let t = Instant::now();
        events.extend(h.poll());
        let dauer = t.elapsed();
        // Budget 200 ms, eine angefangene Zustellung darf bis zu ihrer Frist (200 ms) laufen.
        assert!(
            dauer < Duration::from_millis(700),
            "Bild {bilder}: {dauer:?}"
        );
    }
    assert_eq!(bilder, 3, "drei Bilder hintereinander über dem Budget");
    let reason = planora_i18n::tr!("plugin-too-much-work");
    assert_eq!(state(&h, FAELLE), EntryState::Failed(reason.clone()));
    assert!(events.iter().any(
        |e| matches!(e, HostEvent::Failed { plugin, reason: r } if plugin == FAELLE && *r == reason)
    ));
    assert_eq!(failures(&h, FAELLE), 1);
    aufraeumen(&p);
}

/// WASI-Sockets sind aus: der Versuch scheitert als Fehler des Plug-ins (kein Ausfall), und es
/// kommt keine Verbindung an.
#[test]
fn sockets_are_refused() {
    let listener = std::net::TcpListener::bind("127.0.0.1:47613").ok();
    if let Some(l) = &listener {
        l.set_nonblocking(true).unwrap();
    }
    let (mut h, p) = gestartet("socket", &["test-faelle"]);
    h.run_command(FAELLE, "socket");
    let events = h.poll();
    let text = events
        .iter()
        .find_map(|e| match e {
            HostEvent::Message { plugin, text, .. } if plugin == FAELLE => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("keine Fehlermeldung: {events:?}"));
    assert!(text.starts_with("socket: "), "{text}");
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    if let Some(l) = &listener {
        std::thread::sleep(Duration::from_millis(100));
        assert!(l.accept().is_err(), "eine Verbindung kam an");
    }
    aufraeumen(&p);
}

/// Ein neuer Stand (andere Version) beginnt ohne Ausfälle, auch ein angehaltenes Plug-in.
#[test]
fn new_version_resets_failures() {
    let (mut h, p) = gestartet("ausfaelle-neu", &["test-faelle"]);
    for _ in 0..3 {
        h.run_command(FAELLE, "panik");
        h.poll();
        h.restart(FAELLE);
        warten(&mut h, FAELLE);
    }
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);
    // Gleicher Stand: bleibt angehalten.
    h.rescan(&BTreeMap::new());
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);
    manifest_aendern(&p, "version = \"0.1.0\"", "version = \"0.2.0\"");
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(failures(&h, FAELLE), 0);
    aufraeumen(&p);
}

/// Die Laufzeit (mit Epochen-Takt) entsteht erst mit dem ersten Plug-in.
#[test]
fn runtime_only_with_plugins() {
    let leer = orte("ohne-laufzeit", &[]);
    let mut h = PluginHost::new(leer.clone());
    h.start_all(&BTreeMap::new());
    h.poll();
    assert!(!h.runtime_started());
    aufraeumen(&leer);

    let p = orte("mit-laufzeit", &["beispiel-hallo"]);
    let mut h = PluginHost::new(p.clone());
    assert!(!h.runtime_started());
    h.start_all(&BTreeMap::new());
    assert!(h.runtime_started());
    warten(&mut h, HALLO);
    aufraeumen(&p);
}

/// Beim Start fallen Einträge des Zwischenspeichers weg, die keinem gefundenen Plug-in gehören.
#[test]
fn cache_pruned_at_start() {
    let p = orte("cache-aufraeumen", &["beispiel-hallo"]);
    let mut h = PluginHost::new(p.clone());
    h.start_all(&BTreeMap::new());
    warten(&mut h, HALLO);
    drop(h);
    let eintraege = |p: &Places| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(&p.cache)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".cwasm"))
            .collect();
        v.sort();
        v
    };
    let eigen = eintraege(&p);
    assert_eq!(eigen.len(), 1, "{eigen:?}");
    let fremd = p
        .cache
        .join(format!("{}-0011223344556677.cwasm", "ab".repeat(32)));
    std::fs::write(&fremd, b"alt").unwrap();
    let mut h = PluginHost::new(p.clone());
    h.start_all(&BTreeMap::new());
    warten(&mut h, HALLO);
    let start = Instant::now();
    while fremd.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "nicht aufgeräumt"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(eintraege(&p), eigen, "der eigene Eintrag bleibt");
    drop(h);
    aufraeumen(&p);
}

/// Rechte gehen beim Ersetzen nur auf ein Plug-in derselben Herkunft über: aus einer Datei
/// installiert, dann über einen Repo-Link ersetzt (`herkunft.json`), fällt `net`.
#[test]
fn replace_from_other_source_drops_rights() {
    let (mut h, p) = mit_netzrecht("ersetzen-herkunft", false);
    manifest_aendern(&p, "version = \"0.1.0\"", "version = \"0.2.0\"");
    places::write_provenance(
        &p.user.join(FAELLE),
        &places::Provenance {
            link: "https://github.com/jemand/faelle".into(),
            tag: "v0.2.0".into(),
        },
    )
    .unwrap();
    h.rescan(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(net(&h), None, "andere Herkunft");
    aufraeumen(&p);
}

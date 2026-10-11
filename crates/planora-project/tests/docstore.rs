use planora_feature::{Document, FeatureKind, Snapshot};
use planora_math::DVec3;
use planora_project::ProjectError;
use planora_project::docstore::*;
use planora_project::fsutil::new_id;
use std::path::PathBuf;

fn dir() -> PathBuf {
    std::env::temp_dir().join(format!("planora-doc-{}.pcad", new_id()))
}

/// Dokument mit `n` Quadern; unterscheidbar durch die Zahl der Features.
fn snap(n: usize) -> Snapshot {
    let mut d = Document::new();
    for i in 0..n {
        d.add_feature(
            format!("Q{i}"),
            FeatureKind::Box {
                size: DVec3::splat(1.0 + i as f64),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    }
    d.snapshot()
}

fn count(s: &Snapshot) -> usize {
    s.features.len()
}

#[test]
fn create_save_and_load_back_versions() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "Anfang").unwrap();
    assert_eq!(st.info().versions.len(), 1);
    assert_eq!(st.info().active_branch, MAIN_BRANCH);
    let v2 = st.save(Some(1), &snap(2), "Zwei", None).unwrap();
    let v3 = st.save(Some(v2), &snap(3), "", None).unwrap();
    assert_eq!((v2, v3), (2, 3));
    assert_eq!(st.info().tip(MAIN_BRANCH), Some(3));
    assert_eq!(count(&st.load_version(2).unwrap()), 2);
    st.info().check().unwrap();
    drop(st);
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert!(report.orphans.is_empty() && report.missing.is_empty());
    assert_eq!(st.info().versions.len(), 3);
    assert_eq!(st.info().version(2).unwrap().comment, "Zwei");
    assert_eq!(count(&st.load_version(3).unwrap()), 3);
}

#[test]
fn saving_on_an_old_version_needs_a_branch_name() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    st.save(Some(2), &snap(3), "", None).unwrap();
    assert!(matches!(
        st.save(Some(1), &snap(9), "", None),
        Err(ProjectError::NeedsBranch)
    ));
    assert_eq!(st.info().versions.len(), 3, "nichts geändert");
    let v4 = st
        .save(Some(1), &snap(9), "Variante", Some("Variante A"))
        .unwrap();
    assert_eq!(st.info().version(v4).unwrap().parent, Some(1));
    assert_eq!(st.info().version(v4).unwrap().branch, "Variante A");
    assert_eq!(st.info().tip("Variante A"), Some(v4));
    assert_eq!(st.info().tip(MAIN_BRANCH), Some(3));
    // Speichern auf der Spitze des Nebenzweigs hängt dort an
    let v5 = st.save(Some(v4), &snap(8), "", None).unwrap();
    assert_eq!(st.info().version(v5).unwrap().branch, "Variante A");
    for bad in [
        "",
        "   ",
        "Haupt",
        "Variante A",
        &"x".repeat(MAX_BRANCH_CHARS + 1),
        "a\u{0}b",
    ] {
        assert!(
            st.save(Some(1), &snap(1), "", Some(bad)).is_err(),
            "{bad:?}"
        );
    }
    st.info().check().unwrap();
}

#[test]
fn restore_appends_a_copy_to_the_branch_tip() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    st.save(Some(2), &snap(3), "", None).unwrap();
    let v = st.restore(1, MAIN_BRANCH).unwrap();
    assert_eq!(v, 4);
    assert_eq!(st.info().version(4).unwrap().parent, Some(3));
    assert_eq!(
        st.info().version(4).unwrap().comment,
        "Wiederhergestellt aus v1"
    );
    assert_eq!(count(&st.load_version(4).unwrap()), 1);
    assert!(st.restore(99, MAIN_BRANCH).is_err());
    assert!(st.restore(1, "Gibt es nicht").is_err());
}

#[test]
fn comments_and_active_branch() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", Some("B")).unwrap();
    st.set_comment(2, "  neu  \nzweite Zeile").unwrap();
    assert_eq!(
        st.info().version(2).unwrap().comment,
        "neu  \nzweite Zeile".trim_start()
    );
    assert!(
        st.set_comment(2, &"x".repeat(MAX_COMMENT_CHARS + 1))
            .is_err()
    );
    assert!(st.set_comment(77, "x").is_err());
    st.set_active_branch("B").unwrap();
    assert!(st.set_active_branch("Z").is_err());
    drop(st);
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(st.info().active_branch, "B");
    assert_eq!(st.latest_readable("B"), Some(2));
}

#[test]
fn deletion_rules() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    assert!(st.delete_version(1).is_err(), "die einzige Version bleibt");
    st.save(Some(1), &snap(2), "", None).unwrap(); // v2 Haupt
    st.save(Some(1), &snap(3), "", Some("B")).unwrap(); // v3 Zweig B ab v1
    st.save(Some(3), &snap(4), "", None).unwrap(); // v4 Zweig B
    assert!(st.delete_version(1).is_err(), "hat Kinder");
    assert!(st.delete_version(3).is_err(), "hat Kinder");
    st.delete_version(4).unwrap();
    assert_eq!(st.info().tip("B"), Some(3));
    assert!(
        st.delete_branch("Haupt").is_err(),
        "v1 hat ein Kind in Zweig B"
    );
    assert!(st.delete_branch("Gibt es nicht").is_err());
    assert!(st.delete_branch("B").is_err(), "B ist der aktive Zweig");
    st.set_active_branch("Haupt").unwrap();
    st.delete_branch("B").unwrap();
    assert!(st.info().tip("B").is_none());
    assert_eq!(st.info().versions.len(), 2);
    st.info().check().unwrap();
    // Zweig mit Kind außerhalb darf nicht weg: v2 (Haupt) bekommt ein Kind in Zweig C
    st.save(Some(2), &snap(5), "", None).unwrap(); // Haupt
    st.save(Some(2), &snap(6), "", Some("C")).unwrap(); // Zweig C ab v2 (v2 ist nicht mehr Spitze)
    st.set_active_branch("C").unwrap();
    assert!(st.delete_branch("Haupt").is_err(), "v2 hat ein Kind in C");
    drop(st);
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert!(
        report.orphans.is_empty(),
        "Löschen entfernt die Dateien: {:?}",
        report.orphans
    );
    st.info().check().unwrap();
}

#[test]
fn a_crash_between_the_files_leaves_a_valid_old_state_and_an_orphan() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    // dokument.json durch einen Ordner ersetzen: das Ersetzen schlägt fehl, wie bei einem Absturz
    let doc = p.join("dokument.json");
    let saved = std::fs::read(&doc).unwrap();
    std::fs::remove_file(&doc).unwrap();
    std::fs::create_dir(&doc).unwrap();
    assert!(st.save(Some(1), &snap(2), "", None).is_err());
    assert_eq!(
        st.info().versions.len(),
        1,
        "Speicherstand im Arbeitsspeicher unverändert"
    );
    std::fs::remove_dir(&doc).unwrap();
    std::fs::write(&doc, saved).unwrap();
    drop(st);
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(st.info().versions.len(), 1);
    assert_eq!(report.orphans, ["0002.json"]);
    assert_eq!(count(&st.load_version(1).unwrap()), 1);
    // neue Versionsnummern kollidieren nicht mit der verwaisten Datei
    let mut st = st;
    assert_eq!(st.save(Some(1), &snap(3), "", None).unwrap(), 3);
}

#[test]
fn damaged_documents_give_errors_and_reports() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    drop(st);
    let doc = p.join("dokument.json");
    let good = std::fs::read_to_string(&doc).unwrap();
    for bad in [
        String::new(),
        "{".to_string(),
        good.replace("\"format\": 1", "\"format\": 99"),
        good.replace("\"eltern\": 1", "\"eltern\": 7"),
        good.replace("\"Haupt\"", "\"\""),
    ] {
        std::fs::write(&doc, &bad).unwrap();
        assert!(DocStore::open(&p, LockMode::Acquire).is_err(), "{bad}");
    }
    std::fs::write(&doc, &good).unwrap();
    // fehlende und abgeschnittene Versionsdatei
    std::fs::remove_file(p.join("versionen/0002.json")).unwrap();
    let (st, report) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert_eq!(report.missing, [2]);
    assert_eq!(st.latest_readable(MAIN_BRANCH), Some(1));
    assert!(st.load_version(2).is_err());
    drop(st);
    std::fs::write(p.join("versionen/0001.json"), "{\"format\":1,\"marker\":0").unwrap();
    let (st, _) = DocStore::open(&p, LockMode::Acquire).unwrap();
    assert!(matches!(
        st.load_version(1),
        Err(ProjectError::Corrupt { .. })
    ));
}

#[test]
fn a_second_opener_is_locked_out_until_taken_over() {
    let p = dir();
    let st = DocStore::create(&p, &snap(1), "").unwrap();
    assert!(matches!(
        DocStore::open(&p, LockMode::Acquire),
        Err(ProjectError::Locked { .. })
    ));
    let (b, _) = DocStore::open(&p, LockMode::TakeOver).unwrap();
    drop(st);
    drop(b);
    assert!(DocStore::open(&p, LockMode::Acquire).is_ok());
}

#[test]
fn peek_and_read_version_work_while_the_document_is_locked() {
    let p = dir();
    let mut st = DocStore::create(&p, &snap(1), "").unwrap();
    st.save(Some(1), &snap(2), "", None).unwrap();
    let (info, report) = peek(&p).unwrap();
    assert_eq!(info.versions.len(), 2);
    assert!(report.orphans.is_empty() && report.missing.is_empty());
    assert_eq!(count(&read_version(&p, 2).unwrap()), 2);
    assert!(read_version(&p, 9).is_err());
    assert_eq!(st.info(), &info);
}

#[test]
fn autosave_round_trips_and_is_discarded() {
    let p = dir();
    let st = DocStore::create(&p, &snap(1), "").unwrap();
    assert!(st.read_autosave().unwrap().is_none());
    st.write_autosave(Some(1), &snap(4)).unwrap();
    let a = st.read_autosave().unwrap().unwrap();
    assert_eq!((a.based_on, count(&a.snapshot)), (Some(1), 4));
    assert!(a.time.as_str() >= st.info().latest_time());
    // es erzeugt keine Version
    assert_eq!(st.info().versions.len(), 1);
    st.discard_autosave();
    assert!(st.read_autosave().unwrap().is_none());
    std::fs::write(p.join(".autosave.json"), "kaputt").unwrap();
    assert!(st.read_autosave().is_err());
}

mod random {
    use super::*;
    use proptest::prelude::*;

    #[derive(Clone, Debug)]
    enum Op {
        Save(usize, Option<u8>),
        Restore(usize, usize),
        Comment(usize),
        DelVersion(usize),
        DelBranch(usize),
        Activate(usize),
        Reopen,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0usize..10, proptest::option::of(0u8..4)).prop_map(|(v, b)| Op::Save(v, b)),
            (0usize..10, 0usize..4).prop_map(|(v, b)| Op::Restore(v, b)),
            (0usize..10).prop_map(Op::Comment),
            (0usize..10).prop_map(Op::DelVersion),
            (0usize..4).prop_map(Op::DelBranch),
            (0usize..4).prop_map(Op::Activate),
            Just(Op::Reopen),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]
        #[test]
        fn random_operations_keep_the_version_tree_valid(ops in proptest::collection::vec(op(), 1..25)) {
            let p = dir();
            let mut st = DocStore::create(&p, &snap(1), "").unwrap();
            for o in ops {
                let nums: Vec<u32> = st.info().versions.iter().map(|v| v.number).collect();
                let branches: Vec<String> = st.info().branches.keys().cloned().collect();
                let pick = |i: usize| nums[i % nums.len()];
                let bpick = |i: usize| branches[i % branches.len()].clone();
                match o {
                    Op::Save(v, b) => { let _ = st.save(Some(pick(v)), &snap(2), "x", b.map(|n| format!("Z{n}")).as_deref()); }
                    Op::Restore(v, b) => { let _ = st.restore(pick(v), &bpick(b)); }
                    Op::Comment(v) => { let _ = st.set_comment(pick(v), "k"); }
                    Op::DelVersion(v) => { let _ = st.delete_version(pick(v)); }
                    Op::DelBranch(b) => { let _ = st.delete_branch(&bpick(b)); }
                    Op::Activate(b) => { let _ = st.set_active_branch(&bpick(b)); }
                    Op::Reopen => {
                        let before = st.info().clone();
                        drop(st);
                        let (s2, rep) = DocStore::open(&p, LockMode::Acquire).unwrap();
                        prop_assert_eq!(&before, s2.info());
                        prop_assert!(rep.missing.is_empty());
                        st = s2;
                    }
                }
                prop_assert!(st.info().check().is_ok(), "{:?}", st.info().check());
                for v in &st.info().versions {
                    prop_assert!(st.load_version(v.number).is_ok());
                }
            }
        }
    }
}

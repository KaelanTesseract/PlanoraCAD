//! Manifest und Paket (Plug-ins Etappe 1, Task 1).

use planora_exchange::{ZipWriter, crc32};
use planora_plugin_host::manifest::{Manifest, Right};
use planora_plugin_host::package::{Package, PackageError, Signature};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

const MANIFEST: &str = r#"
id = "planora.beispiel-hallo"
name = "Hallo"
version = "0.1.0"
api = "0.1"
author = "Dennis Guse"
license = "GPL-3.0-only"
description = "Sagt Hallo."
rights = ["net"]
net-hosts = ["example.org"]
"#;

/// Manifest mit einer Zeile ersetzt (`alt` kommt in [`MANIFEST`] vor).
fn changed(alt: &str, neu: &str) -> String {
    assert!(MANIFEST.contains(alt), "{alt}");
    MANIFEST.replace(alt, neu)
}

#[test]
fn manifest_ok() {
    let m = Manifest::parse(MANIFEST).unwrap();
    assert_eq!(m.id, "planora.beispiel-hallo");
    assert_eq!(m.name, "Hallo");
    assert_eq!(m.version, semver::Version::new(0, 1, 0));
    assert_eq!(m.api, "0.1");
    assert_eq!(m.author, "Dennis Guse");
    assert_eq!(m.license, "GPL-3.0-only");
    assert_eq!(m.description, "Sagt Hallo.");
    assert_eq!(m.rights, BTreeSet::from([Right::Net]));
    assert_eq!(m.net_hosts, vec!["example.org".to_string()]);
    assert_eq!(m.memory_mb, 512);
    assert_eq!(Right::Net.id(), "net");
    assert_eq!(Right::FsFolder.id(), "fs-folder");
    assert_eq!(Right::ModelRead.id(), "model-read");
    assert_eq!(Right::ModelWrite.id(), "model-write");
}

#[test]
fn manifest_unknown_keys_are_tolerated() {
    let text = format!("{MANIFEST}\nzukunft = 1\n");
    assert!(Manifest::parse(&text).is_ok());
}

#[test]
fn manifest_rejects() {
    let long_id = format!("id = \"a.{}\"", "b".repeat(63));
    let cases: Vec<(String, &str)> = vec![
        (
            changed("planora.beispiel-hallo", "Planora.Hallo"),
            "Kennung",
        ),
        (changed("planora.beispiel-hallo", "planora"), "Kennung"),
        (
            changed("id = \"planora.beispiel-hallo\"", &long_id),
            "Kennung",
        ),
        (changed("version = \"0.1.0\"", "version = \"1\""), "Version"),
        (changed("[\"net\"]", "[\"kamera\"]"), "kamera"),
        (changed("net-hosts = [\"example.org\"]\n", ""), "net-hosts"),
        (changed("rights = [\"net\"]", "rights = []"), "net-hosts"),
        (changed("example.org", "https://x.org"), "x.org"),
        (changed("example.org", "x.org/a"), "x.org/a"),
        // Zahlen- und Hex-Adressen sind keine Rechnernamen.
        (changed("example.org", "127.0.0.1"), "127.0.0.1"),
        (changed("example.org", "2130706433"), "2130706433"),
        (changed("example.org", "0x7f000001"), "0x7f000001"),
        (changed("example.org", "0X7F.1"), "0X7F.1"),
        (changed("example.org", "example.123"), "example.123"),
        (format!("{MANIFEST}memory-mb = 0\n"), "memory-mb"),
        (format!("{MANIFEST}memory-mb = 2049\n"), "memory-mb"),
        (changed("GPL-3.0-only", ""), "license"),
        ("kein toml [".to_string(), "plugin.toml"),
    ];
    for (text, hint) in cases {
        let err = Manifest::parse(&text).err().unwrap_or_else(|| {
            panic!("sollte abgelehnt werden ({hint}): {text}");
        });
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains(hint), "Meldung nennt {hint}: {msg}");
    }
    // Obergrenze gilt, 2048 ist erlaubt.
    let m = Manifest::parse(&format!("{MANIFEST}memory-mb = 2048\n")).unwrap();
    assert_eq!(m.memory_mb, 2048);
}

/// Ein Eintrag für [`zip_file`].
struct Item<'a> {
    name: &'a str,
    data: &'a [u8],
    deflate: bool,
}

/// Schreibt eine ZIP-Datei mit beliebigen Namen (auch unzulässigen); optional deflate.
fn zip_file(items: &[Item]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for it in items {
        let packed = if it.deflate {
            miniz_oxide::deflate::compress_to_vec(it.data, 6)
        } else {
            it.data.to_vec()
        };
        let method: u16 = if it.deflate { 8 } else { 0 };
        let crc = crc32(it.data);
        let offset = out.len() as u32;
        let head = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes());
            }
            h.extend_from_slice(&20u16.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(&method.to_le_bytes());
            h.extend_from_slice(&0u32.to_le_bytes());
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            h.extend_from_slice(&(it.data.len() as u32).to_le_bytes());
            h.extend_from_slice(&(it.name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            if central {
                h.extend_from_slice(&[0u8; 6]);
                h.extend_from_slice(&0u32.to_le_bytes());
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(it.name.as_bytes());
            h
        };
        out.extend(head(0x0403_4b50, false));
        out.extend(&packed);
        central.extend(head(0x0201_4b50, true));
    }
    let dir_at = out.len() as u32;
    out.extend(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&dir_at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn it<'a>(name: &'a str, data: &'a [u8]) -> Item<'a> {
    Item {
        name,
        data,
        deflate: true,
    }
}

/// Ein vollständiges Paket mit dem `ZipWriter` gebaut.
fn full_package() -> Vec<u8> {
    let mut w = ZipWriter::new(Vec::new());
    let files: [(&str, &[u8]); 5] = [
        ("plugin.toml", MANIFEST.as_bytes()),
        ("plugin.wasm", b"\0asm\x01\0\0\0"),
        ("languages/de.ftl", b"hallo = Hallo\n"),
        ("icons/a.svg", b"<svg/>"),
        ("signature", b"sig"),
    ];
    for (name, data) in files {
        w.add_entry(name, &|o| o.write_all(data)).unwrap();
    }
    w.finish().unwrap()
}

#[test]
fn package_ok() {
    let p = Package::read(&full_package()).unwrap();
    assert_eq!(p.manifest.id, "planora.beispiel-hallo");
    assert_eq!(p.wasm, b"\0asm\x01\0\0\0");
    assert_eq!(p.languages["de"], "hallo = Hallo\n");
    assert_eq!(p.icons["a.svg"], b"<svg/>");
    assert_eq!(p.signature, Signature::Present);
}

#[test]
fn package_ok_deflate_without_signature() {
    let data = zip_file(&[
        it("plugin.toml", MANIFEST.as_bytes()),
        it("plugin.wasm", b"\0asm\x01\0\0\0"),
    ]);
    let p = Package::read(&data).unwrap();
    assert_eq!(p.signature, Signature::None);
    assert!(p.languages.is_empty());
    assert!(p.icons.is_empty());
}

fn wasm_ok() -> Item<'static> {
    it("plugin.wasm", b"\0asm\x01\0\0\0")
}

#[test]
fn package_rejects() {
    // Fehlende Pflichtdateien.
    let e = Package::read(&zip_file(&[wasm_ok()])).err().unwrap();
    assert!(
        matches!(e, PackageError::Missing(ref n) if n == "plugin.toml"),
        "{e:?}"
    );
    let e = Package::read(&zip_file(&[it("plugin.toml", MANIFEST.as_bytes())]))
        .err()
        .unwrap();
    assert!(
        matches!(e, PackageError::Missing(ref n) if n == "plugin.wasm"),
        "{e:?}"
    );
    // Unzulässige Namen.
    for name in ["../x", "/x", "C:\\x", "a\\..\\b"] {
        let data = zip_file(&[
            it("plugin.toml", MANIFEST.as_bytes()),
            wasm_ok(),
            it(name, b"x"),
        ]);
        let e = Package::read(&data).err().unwrap();
        assert!(matches!(e, PackageError::Zip(_)), "{name}: {e:?}");
        assert!(!e.to_string().is_empty());
    }
    // Zu viele Einträge.
    let names: Vec<String> = (0..1001).map(|i| format!("icons/{i}.svg")).collect();
    let mut items: Vec<Item> = names.iter().map(|n| it(n, b"x")).collect();
    items.push(it("plugin.toml", MANIFEST.as_bytes()));
    items.push(wasm_ok());
    let e = Package::read(&zip_file(&items)).err().unwrap();
    assert!(matches!(e, PackageError::Zip(_)), "{e:?}");
    // Kaputtes Manifest.
    let data = zip_file(&[it("plugin.toml", b"id = 1"), wasm_ok()]);
    assert!(matches!(
        Package::read(&data).err().unwrap(),
        PackageError::Manifest(_)
    ));
    // Kein ZIP.
    assert!(matches!(
        Package::read(b"kein zip").err().unwrap(),
        PackageError::Zip(_)
    ));
}

#[test]
fn package_too_large_rejected() {
    // Paket über 64 MB (Eingabelänge), schon vor dem Lesen abgelehnt.
    let big = vec![0u8; (64 << 20) + 1];
    assert!(matches!(
        Package::read(&big).err().unwrap(),
        PackageError::TooLarge
    ));
    // Zip-Bombe: 33 MB Nullen als Wasm, deflate gepackt winzig, entpackt über 32 MB.
    let zeros = vec![0u8; 33 << 20];
    let data = zip_file(&[
        it("plugin.toml", MANIFEST.as_bytes()),
        it("plugin.wasm", &zeros),
    ]);
    assert!(data.len() < (1 << 20));
    let e = Package::read(&data).err().unwrap();
    assert!(matches!(e, PackageError::Zip(_)), "{e:?}");
}

fn work(name: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("package-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn listing(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(
                    p.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn unpack_writes_only_known() {
    let data = zip_file(&[
        it("plugin.toml", MANIFEST.as_bytes()),
        wasm_ok(),
        it("languages/de.ftl", b"a = b\n"),
        it("icons/a.svg", b"<svg/>"),
        it("signature", b"sig"),
        it("boese.exe", b"MZ"),
        it("languages/tief/x.ftl", b"x"),
        it("anderes/datei.txt", b"x"),
    ]);
    let p = Package::read(&data).unwrap();
    let dir = work("known");
    p.unpack_to(&dir).unwrap();
    assert_eq!(
        listing(&dir),
        vec![
            "icons/a.svg",
            "languages/de.ftl",
            "plugin.toml",
            "plugin.wasm",
            "signature"
        ]
    );
    assert_eq!(fs::read(dir.join("plugin.wasm")).unwrap(), p.wasm);
    // Ohne Signatur wird keine geschrieben.
    let data = zip_file(&[it("plugin.toml", MANIFEST.as_bytes()), wasm_ok()]);
    let dir = work("ohne-signatur");
    Package::read(&data).unwrap().unpack_to(&dir).unwrap();
    assert_eq!(listing(&dir), vec!["plugin.toml", "plugin.wasm"]);
}

//! Bettet Programmsymbol und Manifest (`asInvoker`) in die .exe ein (nur Windows) und, wenn
//! `PLANORA_SETUP_ENGINE` auf eine vorhandene Datei zeigt, den Inno-Setup-Motor
//! (`cfg(has_engine)`, Pfad in `PLANORA_SETUP_ENGINE_PATH`). Ist die Variable gesetzt, die Datei
//! aber nicht da, bricht der Bau ab; ohne Variable entsteht eine Hülle ohne Motor.

use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_engine)");
    println!("cargo:rerun-if-env-changed=PLANORA_SETUP_ENGINE");
    if let Some(engine) = std::env::var_os("PLANORA_SETUP_ENGINE") {
        let engine = Path::new(&engine);
        println!("cargo:rerun-if-changed={}", engine.display());
        if !engine.is_file() {
            panic!(
                "PLANORA_SETUP_ENGINE ist gesetzt, aber die Datei fehlt: {}",
                engine.display()
            );
        }
        let abs = std::fs::canonicalize(engine).expect("Pfad des Motors nicht auflösbar");
        println!(
            "cargo:rustc-env=PLANORA_SETUP_ENGINE_PATH={}",
            abs.display()
        );
        println!("cargo:rustc-cfg=has_engine");
    }

    println!("cargo:rerun-if-changed=planoracad-setup.rc");
    println!("cargo:rerun-if-changed=planoracad-setup.manifest");
    println!("cargo:rerun-if-changed=../../assets/logo/planoracad.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Das Manifest steckt in der .rc; der MSVC-Linker darf kein eigenes dazulegen, sonst
        // gäbe es zwei Manifest-Ressourcen.
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bins=/MANIFEST:NO");
        }
        embed_resource::compile("planoracad-setup.rc", embed_resource::NONE)
            .manifest_required()
            .expect("Programmsymbol und Manifest konnten nicht eingebettet werden");
    }
}

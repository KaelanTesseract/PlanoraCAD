//! Baut aus einem Plug-in-Ordner und seiner `.wasm`-Komponente ein `.pcplugin`-Paket und schreibt
//! den Quell-Hash daneben. Wird von `tools/build-plugins.ps1`/`.sh` aufgerufen:
//!
//! `cargo run -p planora-plugin-host --example pack -- <plugin-ordner> <komponente.wasm> <ziel-ordner>`

#[path = "../tests/support/source_hash.rs"]
mod source_hash;

use planora_exchange::ZipWriter;
use std::path::Path;

fn run(dir: &Path, wasm: &Path, out_dir: &Path) -> Result<(), String> {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Ordnername fehlt")?
        .to_string();
    let io = |e: std::io::Error| e.to_string();
    let toml = std::fs::read(dir.join("plugin.toml")).map_err(io)?;
    let wasm = std::fs::read(wasm).map_err(io)?;
    let mut sprachen = Vec::new();
    for e in std::fs::read_dir(dir.join("languages")).map_err(io)? {
        let e = e.map_err(io)?;
        let file = e.file_name().to_string_lossy().into_owned();
        if file.ends_with(".ftl") {
            sprachen.push((
                format!("languages/{file}"),
                std::fs::read(e.path()).map_err(io)?,
            ));
        }
    }
    sprachen.sort();
    let mut zip = ZipWriter::new(Vec::new());
    let mut add = |n: &str, d: &[u8]| {
        zip.add_entry(n, &|o| o.write_all(d))
            .map_err(|e| e.to_string())
    };
    add("plugin.toml", &toml)?;
    add("plugin.wasm", &wasm)?;
    for (n, d) in &sprachen {
        add(n, d)?;
    }
    let bytes = zip.finish().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out_dir).map_err(io)?;
    std::fs::write(out_dir.join(format!("{name}.pcplugin")), &bytes).map_err(io)?;
    let hash = source_hash::source_hash(dir).map_err(io)?;
    std::fs::write(
        out_dir.join(format!("{name}.quelle-sha256")),
        format!("{hash}\n"),
    )
    .map_err(io)?;
    println!("{name}.pcplugin: {} Bytes", bytes.len());
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, wasm, out] = args.as_slice() else {
        eprintln!("Aufruf: pack <plugin-ordner> <komponente.wasm> <ziel-ordner>");
        std::process::exit(2);
    };
    if let Err(e) = run(Path::new(dir), Path::new(wasm), Path::new(out)) {
        eprintln!("Fehler: {e}");
        std::process::exit(1);
    }
}

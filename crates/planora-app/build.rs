//! Bettet das Programmsymbol in die .exe ein (Explorer, Taskleiste). Nur unter Windows.

fn main() {
    println!("cargo:rerun-if-changed=planoracad.rc");
    println!("cargo:rerun-if-changed=../../assets/logo/planoracad.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("planoracad.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Programmsymbol konnte nicht eingebettet werden");
    }
}

//! Der Vertrag zwischen PlanoraCAD und seinen Plug-ins (WebAssembly-Komponenten).
//!
//! Der Vertrag liegt als WIT-Datei in `wit/plugin.wit` (Paket `planora:plugin@0.1.0`). Das Programm
//! (`planora-plugin-host`) und die Plug-ins erzeugen ihre Bindungen beide aus dieser Datei.

/// Die Fassung der Schnittstelle, die dieses Programm spricht (steht als `api` im Manifest).
pub const API_VERSION: &str = "0.1";

/// Der Text der WIT-Datei.
pub const WIT: &str = include_str!("../wit/plugin.wit");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wit_names_the_package() {
        assert!(WIT.starts_with("package planora:plugin@0.1.0;"));
        assert!(WIT.contains("world plugin"));
    }
}

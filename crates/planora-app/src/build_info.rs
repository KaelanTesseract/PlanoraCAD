//! Angaben zum laufenden Programm: Version mit Kanal, Betriebssystem und Grafikkarte. Sie
//! erscheinen im Über-Dialog, in der Protokolldatei und später in Rückmeldungen.

use eframe::egui_wgpu::wgpu::AdapterInfo;
use planora_update::Version;
use std::sync::OnceLock;

/// Text, wenn die Grafikkarte (noch) nicht bekannt ist.
pub const GPU_UNKNOWN: &str = "Grafikkarte unbekannt";

/// Die Grafikkarte, mit der gezeichnet wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: String,
    pub driver: String,
    pub backend: String,
}

/// Was über das laufende Programm bekannt ist.
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub version: Version,
    pub os: String,
    pub gpu: Option<GpuInfo>,
}

static GPU: OnceLock<GpuInfo> = OnceLock::new();

/// Merkt sich die Grafikkarte (nur der erste Aufruf zählt).
pub fn set_gpu(info: GpuInfo) {
    let _ = GPU.set(info);
}

/// Grafikkartendaten aus den Angaben des Adapters.
pub fn gpu_from_adapter(info: &AdapterInfo) -> GpuInfo {
    let driver = format!("{} {}", info.driver.trim(), info.driver_info.trim());
    let driver = driver.trim();
    GpuInfo {
        name: info.name.clone(),
        driver: if driver.is_empty() {
            "unbekannt".to_string()
        } else {
            driver.to_string()
        },
        backend: format!("{:?}", info.backend),
    }
}

/// Die aktuellen Angaben.
pub fn build_info() -> BuildInfo {
    BuildInfo {
        version: crate::running_version(),
        os: os_text(),
        gpu: GPU.get().cloned(),
    }
}

impl BuildInfo {
    /// Etwa „0.2.0-dev.1 (Dev)“.
    pub fn version_line(&self) -> String {
        format!("{} ({})", self.version, self.version.channel().label())
    }

    /// Etwa „System: Windows 11 (Build 26100)“.
    pub fn system_line(&self) -> String {
        format!("System: {}", self.os)
    }

    /// Etwa „Name · Treiber X · Vulkan“, ohne Grafikkarte [`GPU_UNKNOWN`].
    pub fn gpu_line(&self) -> String {
        match &self.gpu {
            Some(g) => format!("{} · Treiber {} · {}", g.name, g.driver, g.backend),
            None => GPU_UNKNOWN.to_string(),
        }
    }
}

/// Name von Windows aus der Versionsnummer.
pub fn windows_name(major: u32, minor: u32, build: u32) -> String {
    if major == 10 && minor == 0 {
        let name = if build >= 22000 { "11" } else { "10" };
        format!("Windows {name} (Build {build})")
    } else {
        format!("Windows {major}.{minor} (Build {build})")
    }
}

/// Name des Betriebssystems des Rechners.
pub fn os_text() -> String {
    #[cfg(windows)]
    {
        if let Some((major, minor, build)) = windows_version() {
            return windows_name(major, minor, build);
        }
        "Windows".to_string()
    }
    #[cfg(not(windows))]
    {
        if let Ok(text) = std::fs::read_to_string("/etc/os-release") {
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                    let v = v.trim().trim_matches('"').trim();
                    if !v.is_empty() {
                        return v.to_string();
                    }
                }
            }
        }
        std::env::consts::OS.to_string()
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_version() -> Option<(u32, u32, u32)> {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
    // SAFETY: Die Struktur ist vollständig mit Nullen gefüllt und ihre Größe gesetzt, wie
    // `RtlGetVersion` es verlangt; die Funktion schreibt nur in diese Struktur.
    unsafe {
        let mut info: OSVERSIONINFOW = std::mem::zeroed();
        info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        if RtlGetVersion(&mut info) == 0 {
            Some((info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber))
        } else {
            None
        }
    }
}

//! `build_info`: Version mit Kanal, Systemname, Grafikkarte; Windows-Namen und Adapterdaten.

use eframe::egui_wgpu::wgpu::{AdapterInfo, Backend, DeviceType};
use planora_app::build_info::{
    self, BuildInfo, GpuInfo, gpu_from_adapter, gpu_unknown, os_text, windows_name,
};

#[test]
fn windows_names() {
    assert_eq!(windows_name(10, 0, 26100), "Windows 11 (Build 26100)");
    assert_eq!(windows_name(10, 0, 19045), "Windows 10 (Build 19045)");
    assert_eq!(windows_name(10, 0, 21999), "Windows 10 (Build 21999)");
    assert_eq!(windows_name(10, 0, 22000), "Windows 11 (Build 22000)");
    assert_eq!(windows_name(6, 3, 9600), "Windows 6.3 (Build 9600)");
}

#[test]
fn version_line_has_channel() {
    let info = build_info::build_info();
    let v = planora_app::running_version();
    assert_eq!(
        info.version_line(),
        format!("{v} ({})", v.channel().label())
    );
}

#[test]
fn gpu_line_without_gpu_is_unknown() {
    let info = BuildInfo {
        version: planora_app::running_version(),
        os: "Test".into(),
        gpu: None,
    };
    assert_eq!(info.gpu_line(), gpu_unknown().as_str());
    assert_eq!(info.system_line(), "System: Test");
}

#[test]
fn gpu_line_with_gpu_lists_name_driver_backend() {
    let info = BuildInfo {
        version: planora_app::running_version(),
        os: "Test".into(),
        gpu: Some(GpuInfo {
            name: "Testkarte".into(),
            driver: "1.2.3".into(),
            backend: "Vulkan".into(),
        }),
    };
    assert_eq!(info.gpu_line(), "Testkarte · Treiber 1.2.3 · Vulkan");
}

#[test]
fn gpu_from_adapter_fills_unknown_driver() {
    let mut a = AdapterInfo::new(DeviceType::DiscreteGpu, Backend::Vulkan);
    a.name = "Testkarte".into();
    let g = gpu_from_adapter(&a);
    assert_eq!(g.name, "Testkarte");
    assert_eq!(g.driver, "unbekannt");
    assert_eq!(g.backend, "Vulkan");
    a.driver = "NVIDIA".into();
    a.driver_info = "555.1".into();
    assert_eq!(gpu_from_adapter(&a).driver, "NVIDIA 555.1");
    a.driver.clear();
    assert_eq!(gpu_from_adapter(&a).driver, "555.1");
}

#[test]
fn build_info_uses_running_version() {
    assert_eq!(
        build_info::build_info().version,
        planora_app::running_version()
    );
}

#[test]
fn os_text_is_not_empty() {
    assert!(!os_text().trim().is_empty());
}

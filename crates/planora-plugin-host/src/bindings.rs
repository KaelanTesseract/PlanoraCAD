//! Die Bindungen zum Vertrag `planora:plugin@0.1.0`, erzeugt aus `planora-plugin-api/wit/plugin.wit`.

wasmtime::component::bindgen!({
    path: "../planora-plugin-api/wit",
    world: "plugin",
    additional_derives: [PartialEq],
});

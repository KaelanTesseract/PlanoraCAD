use planora_app::dock::{Dock, Pane};

#[test]
fn projects_and_versions_can_be_hidden_and_shown() {
    let mut d = Dock::new();
    assert!(d.is_visible(Pane::Projects));
    assert!(d.is_visible(Pane::Versions));
    d.set_visible(Pane::Projects, false);
    assert!(!d.is_visible(Pane::Projects));
    assert!(d.is_visible(Pane::Versions));
    d.set_visible(Pane::Projects, true);
    assert!(d.is_visible(Pane::Projects));
    // die festen Bereiche lassen sich nicht ausblenden
    d.set_visible(Pane::Browser, false);
    assert!(d.is_visible(Pane::Browser));
}

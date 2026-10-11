//! Anordnung der Bereiche (Docking mit `egui_tiles`).

use egui_tiles::{Container, Tile, TileId, Tiles, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pane {
    Projects,
    Browser,
    Viewport,
    Properties,
    Versions,
}

pub struct Dock {
    pub tree: Tree<Pane>,
    /// Tab-Container des Bereichs „Projekte“ (wird als Ganzes aus- und eingeblendet).
    pub projects: TileId,
    /// Bereich „Versionen“ (ein Tab neben „Eigenschaften“).
    pub versions: TileId,
}

impl Default for Dock {
    fn default() -> Self {
        Dock::new()
    }
}

impl Dock {
    /// Anfangsanordnung: Projekte (14 %), Browser (16 %), 3D-Ansicht (46 %), rechts Tabs
    /// „Eigenschaften“ und „Versionen“ (24 %).
    pub fn new() -> Dock {
        let mut tiles = Tiles::default();
        let projects_pane = tiles.insert_pane(Pane::Projects);
        let projects = tiles.insert_tab_tile(vec![projects_pane]);
        let browser_pane = tiles.insert_pane(Pane::Browser);
        let browser = tiles.insert_tab_tile(vec![browser_pane]);
        let viewport_pane = tiles.insert_pane(Pane::Viewport);
        let viewport = tiles.insert_tab_tile(vec![viewport_pane]);
        let properties_pane = tiles.insert_pane(Pane::Properties);
        let versions = tiles.insert_pane(Pane::Versions);
        let right = tiles.insert_tab_tile(vec![properties_pane, versions]);
        let root = tiles.insert_horizontal_tile(vec![projects, browser, viewport, right]);
        if let Some(Tile::Container(Container::Linear(linear))) = tiles.get_mut(root) {
            // Anteile; Summe 4 (= Anzahl der Kinder): 14 %, 16 %, 46 %, 24 %.
            linear.shares.set_share(projects, 0.56);
            linear.shares.set_share(browser, 0.64);
            linear.shares.set_share(viewport, 1.84);
            linear.shares.set_share(right, 0.96);
        }
        Dock {
            tree: Tree::new("planora_docking", root, tiles),
            projects,
            versions,
        }
    }

    /// Blendet „Projekte“ oder „Versionen“ aus oder ein; andere Bereiche bleiben immer sichtbar.
    pub fn set_visible(&mut self, which: Pane, visible: bool) {
        let id = match which {
            Pane::Projects => self.projects,
            Pane::Versions => self.versions,
            _ => return,
        };
        self.tree.tiles.set_visible(id, visible);
    }

    pub fn is_visible(&self, which: Pane) -> bool {
        match which {
            Pane::Projects => self.tree.tiles.is_visible(self.projects),
            Pane::Versions => self.tree.tiles.is_visible(self.versions),
            _ => true,
        }
    }
}

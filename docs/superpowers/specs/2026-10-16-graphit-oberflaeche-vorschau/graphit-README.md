A dark, near-monochrome UI for a desktop CAD modeller: floating rounded panels hover over a black 3D viewport, almost everything is grey on grey, and colour shows up only where it means something (one blue for "active", three tinted chips for operations, red/green/blue for axes, yellow for selection). Build every screen from the tokens and components below. The visual reference is a Dribbble concept of a redesigned CAD UI; this system describes its style and names no product.

## Principles

- **The model is the hero.** The viewport fills the whole window. Panels float above it with `space-6` inset and never touch the window edge or each other.
- **Grey first, colour as signal.** Surfaces step through `bg-inset` → `bg-viewport` → `bg-panel` → `bg-section` → `bg-field` → `bg-active`. A saturated colour always carries a meaning (state, operation type, axis). Never use colour for decoration.
- **Outline, not shadow.** Every container gets a 1px `border-subtle` outline and a radius. No drop shadows, no gradients, no blur.
- **Quiet at rest, white when it matters.** Resting icons and rows use `text-secondary`; values, titles and anything active switch to `text-primary`.
- **Dense but breathable.** Rows are `size-row` tall, text is 12–13px, but boxes have generous `space-3` padding and rounded corners.

## Layout (reference: 1920 × 1080 window)

| Region | Position | Size | Contents |
| --- | --- | --- | --- |
| Viewport | full window | 100% | `bg-viewport`, floor grid in `viewport-grid`, green `viewport-ground-axis`, model, transform gizmo |
| Outliner panel | left, `space-6` from top and left | `size-panel-width` × ~900px | Account header, SearchField + new-folder IconButton, collapsible SectionHeaders (Solids, Curve…) with TreeRows |
| Top bar | top centre | auto × `size-toolbar` | menu, logo slot, divider, 4 view-mode IconButtons, divider, file-name pill with cloud status + chevron |
| Mode switcher | top right, above properties | `size-panel-width` × `size-toolbar` | 4 IconButtons (wireframe grid, nodes, cube, sphere), first and third may be active |
| View cube orb | viewport, left of the properties panel, near the top | 32px circle | tri-colour `gizmo-x/y/z` faces |
| Properties panel | right, below mode switcher | `size-panel-width` × ~720px | SegmentedControl (Display / Properties), Location / Rotation / Scale VectorFields, Construction Planes |
| Coordinate readout | bottom left, under outliner | auto | X / Y / Z with value chips; unit tinted by axis |
| Transform group + snaps | bottom left, below readout | auto | Toolbar of 6 transform IconButtons, then Grid Snap / Object Snap toggles |
| Command bar + tool dock | bottom centre | ~600px wide | CommandBar with operation Chips, ToolDock of ~11 creation tools + a toggle pill |
| Context menu | at cursor | ~180px | ContextMenu rows with Kbd badges |

Panels sit 24px from the window edges (`space-6`); stacked panels on the same side are separated by `space-3`. When the window shrinks, keep panel widths fixed and let the viewport give up space.

## Colour

- Window and viewport: `bg-viewport`. Panels, toolbars, menus: `bg-panel` with `border-subtle`.
- Inside a panel: section header boxes `bg-section`; search and small buttons `bg-field`; numeric inputs and the command bar are recessed with `bg-inset`.
- Active / selected state of a neutral control: fill `bg-active`, icon or text turns `text-primary`. Never use `accent-primary` for this.
- `accent-primary` is reserved for an *enabled mode* (Grid Snap on), keyboard focus (`focus-ring`) and a selected outliner row's indicator. One or two blue things on screen at most.
- Operation chips use their own pair: `chip-bridge-*` (amber), `chip-split-*` (blue), `chip-boolean-*` (green). Fill is the dark tint, text is the bright tint — never solid bright fills.
- Axes everywhere: X `axis-x`, Y `axis-y`, Z `axis-z`. The view-cube orb uses `gizmo-x/y/z`. Selection in the viewport: `selection-highlight`.

## Typography

Montserrat (Google Fonts, OFL) in 400, 500 and 600 is the only face. Use `heading` for panel titles, `label` for section headers and menu items, `body` for outliner rows and buttons, `value` for numbers in fields and chips, `kbd` for shortcut badges, `axis` for the X/Y/Z letters. Numbers in fields are centred. Sentence case everywhere ("Plan from selection", "Grid Snap" is the exception as a proper toggle name). No all-caps, no letter-spacing tricks.

## Spacing, radius, sizes

- Spacing steps: `space-1` 4, `space-2` 8, `space-3` 12, `space-4` 16, `space-6` 24 px.
- Radius grows with the container: `radius-xs` chips/badges → `radius-sm` inputs/buttons → `radius-md` boxes inside panels → `radius-lg` panels and toolbars → `radius-full` for circles. A nested box's radius is its parent's radius minus the padding between them, never larger than the parent's.
- Fixed sizes live in the `size-*` tokens: rows 24, fields 26, controls 32, headers 40, top bar 48, dock 56, panel width 250 px.

## Iconography

Thin line icons only: 1.5px stroke (`stroke-icon`), round caps and joins, no fills, 16px in rows (`size-icon`), 22px in toolbars (`size-icon-tool`). Colour is `text-secondary` at rest and `text-primary` when hovered or active. The mockup's own icon set is not available; use **Phosphor (Light weight)** or **Lucide** as the closest match — in Rust/egui the `egui-phosphor` crate provides Phosphor as a font. Object types in the outliner get a type icon (solid = cube outline, curve = bezier path) and a trailing "more" icon (circle with three dots).

## States

| State | Neutral control | Row | Field |
| --- | --- | --- | --- |
| Rest | transparent, icon `text-secondary` | text `text-secondary` | `bg-inset`, value `text-primary` |
| Hover | fill `bg-field` | fill `bg-field`, text `text-primary` | outline `border-subtle` → `text-tertiary` |
| Active / selected | fill `bg-active`, icon `text-primary` | fill `bg-active`, text `text-primary` | — |
| Focus (keyboard) | 2px `focus-ring` outside | same | 2px `focus-ring` |
| Disabled | icon `text-muted` | text `text-muted` | value `text-tertiary` |

Transitions: 120 ms ease-out on fill and colour only. Panels never animate position.

## Building it in Rust

The section "Rust / egui" holds a ready-to-paste theme module (colours, fonts, spacing) and the layout recipe with floating `egui::Area`s over a wgpu viewport. Each component card says how it maps to egui widgets.

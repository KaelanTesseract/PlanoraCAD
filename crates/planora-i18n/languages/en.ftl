## Language
language-name = English
decimal-separator = .

## Settings
settings-language = Language
settings-language-not-saved = The language only applies until you close the program: the setting could not be saved.
settings-title = Settings
settings-section-appearance = Appearance
settings-transparency-label = Panel transparency
settings-transparency-not-saved = The transparency only applies until you close the program: the setting could not be saved.
settings-section-updates = Updates
settings-channel-label = Update channel
settings-check-on-start = Check for updates on start
settings-privacy-note = When checking, the program asks GitHub, which can see your IP address.
settings-check-now = Check for updates now
settings-check-running = Checking …
settings-up-to-date = You have the latest version.
settings-check-failed = The check failed (no network?).
settings-not-on-this-system = Updating through the program is not available on this system.
settings-show-notice = Show notice
settings-unskip = Show again
settings-never-checked = never
settings-updates-not-saved = The update setting only applies until you close the program: it could not be saved.
settings-verbose-label = Detailed log
settings-verbose-note = Writes more to the log file on your computer, for example every command and how long it takes. None of it is sent.
settings-verbose-not-saved = The detailed log only applies until you close the program: the setting could not be saved.
settings-section-projects = Projects
settings-directory-label = Project folder
settings-change-directory = Change project folder …
settings-choose-heading = Choose project folder
settings-keep-hint = Your projects stay where they are. The new folder only shows the projects that are in it.
settings-unreadable = The folder cannot be read.
settings-nested = This is already a project or document. Please choose the folder that contains your projects.
settings-not-saved = The new project folder only applies until you close the program: the setting could not be saved.
settings-root-locked = Close all saved documents before you change the project folder.
settings-running-version = Running version: { $version } ({ $channel })
settings-last-check = Last check: { $when }
settings-update-found = New version found: { $version }
settings-skipped = Skipped: version { $version }

## Test
test-hello = Hello
test-args = Hello { $name }
test-count = { $count ->
    [one] { $count } file
   *[other] { $count } files
}
test-only-en = Only in English

## Tastenbelegung
action-new = New document
action-projects = Projects
action-save = Save
action-close = Close document
action-export = Export
action-feedback = Give feedback
action-undo = Undo
action-redo = Redo
action-view-pan = Pan view
action-view-orbit = Rotate view
action-view-zoom = Zoom view
action-extrude-sketch = Extrude sketch
action-fillet = Fillet
action-toggle-visibility = Toggle visibility
action-delete = Delete
action-repeat-last = Repeat last command
action-show-shortcuts = Show keyboard shortcuts
action-sketch-line = Line
action-sketch-rectangle = Rectangle
action-sketch-circle = Circle
action-sketch-project-edge = Copy body edge
action-sketch-repeat-last = Repeat last sketch tool
action-sketch-construction = Toggle construction
action-sketch-show-shortcuts = Show keyboard shortcuts in sketch
action-drawing-dimension = Dimension
action-drawing-balloon = Balloon
area-all = Everywhere
area-model = Model
area-sketch = Sketch
area-drawing = Drawing
keymap-unbound-dash = –
keymap-note-unknown-action = Unknown action "{ $id }" skipped.
keymap-note-fixed = "{ $binding }" for "{ $action }" skipped: { $reason }
keymap-note-invalid = Invalid binding "{ $binding }" for "{ $action }" skipped.
keymap-note-taken = "{ $binding }" for "{ $action }" skipped: already used by "{ $other }".
keymap-note-default-lost = "{ $binding }" no longer applies to "{ $action }": used by "{ $other }" in the file.
keymap-fixed-sketch = in the sketch
keymap-fixed-drawing = in the drawing
keymap-fixed-tab = for switching between controls
keymap-fixed-history-bar = on the history bar
keymap-fixed = "{ $key }" is permanently assigned { $place }.
keymap-notes-more = … and { $count } more

## Tastennamen
key-delete = Delete
key-space = Space
key-escape = Esc
key-enter = Enter
key-backspace = Backspace
key-insert = Insert
key-home = Home
key-end = End
key-page-up = Page Up
key-page-down = Page Down
key-arrow-left = Left Arrow
key-arrow-right = Right Arrow
key-arrow-up = Up Arrow
key-arrow-down = Down Arrow
key-tab = Tab
key-copy = Copy
key-cut = Cut
key-paste = Paste
key-colon = Colon
key-comma = Comma
key-minus = Minus
key-period = Period
key-plus = Plus
key-equals = Equals
key-semicolon = Semicolon
key-backslash = Backslash
key-slash = Slash
key-pipe = Vertical Bar
key-questionmark = Question Mark
key-exclamationmark = Exclamation Mark
key-open-bracket = Open Bracket
key-close-bracket = Close Bracket
key-open-brace = Open Brace
key-close-brace = Close Brace
key-backtick = Backtick
key-quote = Quote
key-browser-back = Browser Back
key-intl-backslash = <> Key
key-ctrl = Ctrl
key-shift = Shift
key-alt = Alt
key-wheel = Mouse wheel
mouse-drag-left = Drag left button
mouse-drag-middle = Drag middle button
mouse-drag-right = Drag right button

## Tastenkürzel (Einstellungen und Übersicht)
keys-section = Keyboard shortcuts
keys-search-hint = Search: action, area or key
keys-help = Choose "Change" and press the new key. Esc cancels.
keys-column-action = Action
keys-column-area = Area
keys-column-bindings = Binding
keys-change = Change
keys-remove = Remove
keys-add = Add
keys-reset = Reset
keys-reset-all = Reset all
keys-press-key = Press a key …
keys-drag-now = Hold a mouse button and drag …
keys-wheel-now = Turn the mouse wheel …
keys-cancel = Cancel
keys-swap = Swap
keys-unbound = not assigned
keys-nothing-found = No action found.
keys-notes-heading = Skipped while loading:
keys-not-saved = The keyboard shortcuts only apply until the program is closed: the setting could not be saved.
keys-conflict = { $binding } is already used by "{ $action }".
keys-taken = { $binding } is a default again and no longer applies to "{ $action }".
keys-swapped = { $binding } no longer applies to "{ $action }".
shortcuts-title = Keyboard shortcuts
shortcuts-search-hint = Search: action or key
shortcuts-nothing-found = No shortcuts found.
shortcuts-to-settings = Change in settings

## Fenster und Dialoge
dialog-close = Close

## Tabs
tab-close = Close

## Menüs
menu-blend-fillet = Fillet …
menu-blend-chamfer = Chamfer …
menu-file-projects = Projects …
menu-export-busy = Please finish the export first.
menu-file-settings = Settings …
menu-view-shortcuts = Keyboard shortcuts …
menu-help = Help
menu-help-feedback = Feedback …
menu-help-faq = Common problems
menu-edit-parameters = Parameters …
menu-file = File
menu-create = Create
menu-drawing = Drawing
menu-edit = Edit
menu-view = View
menu-file-new = New
menu-file-save-as = Save as …
menu-file-export = Export …
menu-file-export-disabled = There is no body yet.
menu-file-import-mesh = Import mesh …
menu-old-version-add = Nothing can be added in an old version.
menu-file-close = Close
menu-answer-prompt-first = Answer the open question first.
menu-file-quit = Quit
menu-view-versions = Versions
menu-create-box = Box …
menu-create-cylinder = Cylinder …
menu-create-sphere = Sphere …
menu-create-cone = Cone …
menu-create-extrude = Extrude sketch …
menu-create-revolve = Revolve sketch …
menu-create-need-sketch = Create a sketch first.
menu-undo-with = Undo: { $label }
menu-redo-with = Redo: { $label }
menu-bool-union = Union …
menu-bool-subtract = Subtract …
menu-bool-intersect = Intersect …
menu-edit-to-end = To end of timeline
menu-edit-to-end-disabled = The whole timeline is already being calculated.
menu-view-iso = Isometric
menu-view-fit-all = Fit all
menu-view-light = Light theme
menu-view-dark = Dark theme
menu-sketch-xy = Sketch on XY plane
menu-sketch-xz = Sketch on XZ plane
menu-sketch-yz = Sketch on YZ plane
menu-sketch-on-face = Sketch on selected face

## Schnellzugriffe
place-favorites-full = There are already 24 favorites. Please remove one first.
place-add-favorite = Add to favorites
place-remove-favorite = Remove from favorites
place-favorite-missing = This folder does not exist. Right-click the button to remove it from the favorites.
place-projects = Projects
place-documents = Documents
place-downloads = Downloads
place-desktop = Desktop
place-pictures = Pictures

## 3D-Ansicht
view-cube-hover = View: { $name }
view-home-label = Home
view-home-hint = Home: default view (isometric)

## Rückfragen
prompt-update-restart-title = Restart for the update
prompt-update-restart-text = PlanoraCAD has to restart to update. Save the unsaved changes in "{ $name }"?
prompt-save-changes-title = Save changes?
prompt-save-changes-text = "{ $name }" has unsaved changes.
prompt-save-version-title = Save version
prompt-save-version-branch-note = This version is not the latest of its branch. The state is saved in a new branch.
prompt-branch = Branch:
prompt-comment = Comment:
prompt-no-project-via-menu = in the "File" menu under "{ $entry }"
prompt-no-project-via-keys = with { $keys }
prompt-no-project = There is no project yet. Please close this with "Cancel", then open the Projects window { $open } and create a project.
prompt-save-as-title = Save as
prompt-no-library = Please close this with "Cancel", then choose the project folder in the "File" menu under "Settings …".
prompt-tree-error = The projects cannot be read: { $error }
prompt-target = Target (project or folder):
prompt-name = Name:
prompt-save-as-disabled = Choose a target and enter a name.
prompt-locked-title = Document locked
prompt-locked-text = The document is currently in use by another program (locked). If that program has crashed, you can take over the lock.
prompt-take-over = Take over lock
prompt-autosave-title = Autosave found
prompt-autosave-text = There is an autosave for this document that is newer than the saved state (for example after a crash).
prompt-restore = Restore
prompt-restore-hint = Load the autosaved state (it then counts as unsaved).
prompt-discard-autosave-hint = Delete the autosave and keep the saved state.
prompt-notice-title = Notice

## Allgemein
common-save = Save
common-discard = Discard
common-cancel = Cancel
common-ok = OK

## Hauptfenster
shell-sketch-keys-hint = Esc cancels, Enter finishes, Delete removes, middle mouse button or Shift + drag pans
shell-old-version-readonly = Nothing can be changed in an old version.
tab-properties = Properties
tab-versions = Versions
chip-union = Union
chip-subtract = Subtract
chip-intersect = Intersect
shell-sketch-mode-info = Sketch mode: { $name } · { $info }
shell-sketch-mode = Sketch mode: { $name }
shell-bodies-note = 
    { $count -> 
        [one] { $count } body
       *[other] { $count } bodies
    } – { $note }
shell-bodies = 
    { $count -> 
        [one] { $count } body
       *[other] { $count } bodies
    }
shell-old-version-banner = Old version v{ $n } – read-only. In the "Versions" area you can restore it or branch from it.

## Befehle
cmd-need-sketch = Choose a sketch with a closed profile first.
cmd-need-body = Choose a body first.
cmd-need-selection = Choose a body or step first.

## Dateiauswahl
chooser-io-not-found = The folder does not exist.
chooser-io-denied = You are not allowed to open this folder.
chooser-io-other = The folder cannot be opened.

## Updates
update-install-now = Update now
update-later = Later
update-skip = Skip this version
update-more-on-github = … and more changes on GitHub.
update-no-notes = No description.
update-preparing = Preparing the update …
update-notes-on-github = See GitHub for what is new.
update-failed-title = Update failed
update-retry = Try again
update-title = New version available: { $version } ({ $channel })
update-running-line = You have version { $version }
update-version-heading = Version { $version }
update-status-hint = Update available: { $version }
update-after-feedback = Something wrong?
update-feedback-button = Send feedback
update-waiting = The update is ready and will wait until you are done.
update-only-windows = Automatic updating is only available on Windows.
update-open-download-page = Open download page
update-pending-not-saved = The update could not be scheduled because the settings cannot be saved. Nothing was installed.
update-still-stopping = The cancelled download is still ending. Please try again in a moment.
update-cancelled = Update cancelled. Without permission from Windows, PlanoraCAD cannot be updated for all users. You keep working in the current version.
update-not-installed-hint = This copy of PlanoraCAD is not installed and cannot update itself. Download the new version from the download page.
update-launch-failed = The update could not be started ({ $detail }). You keep working in the current version.
update-updated-title = Updated to version { $version }
update-restarting = Restarting …
update-restart-box = PlanoraCAD is closing for the update and will start again by itself.
update-failed-short = Update failed
update-downloading-percent = Downloading … { $percent } %
update-downloading-mb = Downloading … { $mb } MB
update-error-cancelled = The update was cancelled.
update-error-network = The new version could not be downloaded ({ $detail }). Please try again later.
update-error-too-large = The downloaded file is larger than expected. Nothing was installed.
update-error-checksum = The downloaded file is damaged (the checksum does not match). Nothing was installed.
update-error-io = The update could not be prepared ({ $detail }). The program keeps running in the current version.
update-detail-foreign-url = Address does not belong to the release
update-detail-download-aborted = Download aborted
update-detail-read-file = Reading file: { $error }
update-detail-no-folder = no folder for the download
update-detail-check-aborted = Check aborted
update-detail-not-installed = not installed or no download

## About
about-title = About PlanoraCAD
about-sources-heading = Bundled fonts and icons
about-source-montserrat = Font Montserrat (SIL Open Font License)
about-source-phosphor = Phosphor Icons (MIT)
about-source-roboto = Font Roboto (Apache-2.0)
about-source-rajdhani = Logo font Rajdhani (SIL Open Font License)
about-version = Version { $version } ({ $channel })
about-gpu = Graphics card: { $gpu }
about-license = License: { $license }
build-gpu-unknown = Graphics card unknown
build-driver-unknown = unknown
build-system = System: { $os }
build-gpu-line = { $name } · Driver { $driver } · { $backend }

## Projects and documents
project-no-root = No project folder has been chosen yet.
project-name-suffix = Document names must not end in ".pcad".
project-nothing-to-undo = There is nothing to undo.
project-nothing-to-redo = There is nothing to redo.
project-export-writing = This is not possible while exporting.
project-untitled = Untitled { $n }
project-read-only-save = This version is read-only. Branch off or restore it first.
project-pick-target = Please choose a project or a folder as the target.
project-no-readable-version = There is no readable version.
project-note-orphans = Found and ignored leftovers of an interrupted save: { $list }.
project-note-missing = Version files are missing: { $list }.
project-note-tip-unreadable = The newest version cannot be read; v{ $v } was loaded.
project-note-autosave-unreadable = The autosave cannot be read and is not used; the file is kept: { $error }
project-autosave-load-failed = The autosave cannot be loaded; the file is kept: { $error }
project-no-such-tab = No such tab.
project-no-location = The document has no location.
project-open-elsewhere = The document is already open in another tab. Continue working there or close the tab.
project-unsaved-changes = There are unsaved changes. Save or discard them first.
project-not-saved = The document is not saved.

## Undo names
undo-change = Change
undo-sheet-info = Change sheet info
undo-parts-text = Change parts list text
undo-param-add = Add parameter
undo-param-delete = Delete parameter
undo-param-change = Change parameter
undo-feature-delete = Delete feature
undo-rename = Rename
undo-suppress = Suppress
undo-unsuppress = Activate
undo-timeline-marker = Set timeline marker
undo-history-marker = Set history marker
undo-move = Move
undo-sketch-edit = Edit sketch
undo-feature-change = Change feature
undo-box = Create box
undo-cylinder = Create cylinder
undo-sphere = Create sphere
undo-cone = Create cone
undo-sketch-create = Create sketch
undo-extrude = Extrude
undo-revolve = Revolve
undo-union = Union
undo-subtract = Subtract
undo-intersect = Intersect
undo-chamfer = Chamfer
undo-fillet = Fillet
undo-mesh-import = Import mesh
undo-step-import = Import STEP
undo-parts-sheet = Create parts list sheet
undo-drawing-create = Create drawing
undo-drawing-delete = Delete drawing
undo-drawing-rename = Rename drawing
undo-section-create = Create section view
undo-view-add = Add view
undo-view-delete = Delete view
undo-view-move = Move view
undo-scale-change = Change scale
undo-section-change = Change section
undo-view-change = Change view
undo-dim-add = Add dimension
undo-dim-delete = Delete dimension
undo-dim-change = Change dimension
undo-balloon-add = Add balloon
undo-balloon-delete = Delete balloon
undo-balloon-move = Move balloon
undo-edge-project = Copy body edge
undo-delete = Delete
undo-line = Draw line
undo-circle = Draw circle
undo-arc = Draw arc
undo-ellipse = Draw ellipse
undo-spline = Draw spline
undo-rectangle = Draw rectangle
undo-draw = Draw
undo-point = Place point
undo-dim-set = Set dimension
undo-constraint = Add constraint
undo-construction = Toggle construction line
undo-formula = Change formula
undo-sketch-change = Change sketch

## Projects window
projects-title = Projects
projects-rename = Rename …
projects-move = Move …
projects-delete = Delete …
projects-scan-lost = Reading the projects ended unexpectedly.
projects-footer-hint = Double-click: open · Right-click: more commands
projects-directory-button = Project folder …
projects-no-bodies = No bodies yet
projects-too-complex = Thumbnail too complex
projects-impossible = Thumbnail not possible
projects-new-project = New project
projects-new-document = New document
projects-new-folder = New folder
projects-refresh = Refresh
projects-empty = No projects yet. Create the first one with "New project".
projects-reading = Reading the projects …
projects-back-hint = Up one level
projects-empty-project = This project is still empty. Create a document or a folder above.
projects-empty-folder = This folder is still empty. Create a document or a folder above.
projects-building = Creating thumbnail …
projects-no-library = No project folder has been chosen yet. Choose the folder for your projects with "Project folder …".
projects-read-error-prefix = { "The project folder cannot be read: " }
projects-close-first = Please close the affected documents first.
projects-targets-pending = Reading the targets …
projects-nothing-selected = Nothing selected.
projects-create = Create
projects-rename-title = Rename
projects-move-title = Move
projects-delete-title = Delete
projects-in-location = In "{ $name }":
projects-new-name-for = New name for "{ $name }":
projects-move-to = Move "{ $name }" to:
projects-choose-target = Choose target
projects-confirm-delete = Move "{ $name }" to the trash?
projects-to-trash = Move to trash
projects-too-many = Only the first 500 are shown.
projects-cancelled = Cancelled
projects-root-missing = "{ $path }" was not found.
projects-changed = Changed { $time }
projects-date-unknown = Date unknown
projects-version = Version { $v }
projects-document-unreadable = Document cannot be read
projects-no-document = No document yet
projects-documents-unreadable = Documents cannot be read
projects-meta-no-bodies = No bodies yet · { $count }
projects-meta-version = Version { $v } · { $count }
projects-documents = { $n ->
    [one] 1 document
   *[other] { $n } documents
}
projects-footer-partial-top = { $shown } of { $total } projects
projects-footer-partial-entries = { $shown } of { $total } entries
projects-count = { $n ->
    [0] No projects
    [one] 1 project
   *[other] { $n } projects
}
projects-folders = { $n ->
    [one] 1 folder
   *[other] { $n } folders
}
projects-empty-level = Empty

## Versions
versions-open-editable = To change anything, open the document for editing first.
versions-not-saved-hint = The document has not been saved yet.
versions-not-saved = This document has not been saved yet.
versions-unreadable = The versions cannot be read.
versions-viewing-old = You are viewing an old version.
versions-restore = Restore
versions-branch = Branch off
versions-unsaved-changes = Changes (not saved)
versions-open = Open
versions-edit-comment = Edit comment …
versions-delete-entry = Delete …
versions-delete-branch-entry = Delete branch …
versions-switch-branch = Switch to this branch
versions-comment-title = Edit comment
versions-comment-for = Comment on v{ $n }
versions-apply = Apply
versions-delete-title = Delete
versions-confirm-delete-version = Delete version v{ $n } permanently?
versions-confirm-delete-branch = Delete branch "{ $name }" with all its versions permanently?

## History bar
history-lock-hint = This body is rolled back. Jump to the end of the history first.
history-global-hint = The whole history is rolled back. Activate it first with "Activate all".
history-body-gone = The body no longer exists
history-no-such-place = This position does not exist in the history
history-pause = Pause
history-play = Play
history-to-start = To the start
history-step-back = One step back
history-step-forward = One step forward
history-to-end = To the end
history-choose-body-first = Choose a body first.
history-before-step-one = Before step 1
history-step-of = Step { $pos } of { $len }
history-activate-all = Activate all
history-global-reset = Entire history rolled back
history-recompute-all = Recompute the entire history
history-global-reset-hint = { $reset }. { $activate }: recompute the entire history
history-choose-body = Choose a body
history-rolled-back = Rolled back
history-double-click-sketch = Double-click opens the sketch.
history-double-click-settings = Double-click opens the settings.
history-edit = Edit
history-rename = Rename
history-activate = Activate
history-suppress = Suppress
history-delete = Delete
history-marker-label = Marker: step { $pos } of { $len }

## Feature tree
tree-status-error = Error
tree-status-skipped = skipped
tree-status-suppressed = suppressed
tree-status-not-computed = not computed
tree-delete-single = "{ $name }" will be deleted.
tree-delete-with-dependents = "{ $name }" and { $count } feature(s) that depend on it will be deleted: { $list }
tree-plan-item = "{ $name }" ({ $kind })
tree-quoted-name = "{ $name }"
tree-face-note = Note: A sketch on a face depends only on the feature that created the face (primitive, extrusion or revolution), not on later operations. It is therefore kept and then lies on the face of that body: { $names }
tree-feature-gone = The feature no longer exists
tree-plan-changed = The effects of deleting have changed; please check the new list
tree-status-line = "{ $name }": { $error }
tree-folder-bodies = Bodies
tree-folder-sketches = Sketches
tree-folder-drawings = Drawings
tree-folder-origin = Origin
tree-empty = No bodies yet. Use "Create" in the menu to add a body or a sketch.
tree-no-sketches = No sketch yet. Use "Create", then "Sketch" to add one.
tree-rolled-back = rolled back
tree-rolled-back-hint = The history of this body is rolled back. You can wind it forward to the end again in the history bar below.
tree-plane-xy = XY plane
tree-plane-xz = XZ plane
tree-plane-yz = YZ plane
tree-suppressed-hint = Suppressed: not computed (right-click, then "Activate")
tree-not-computed-hint = Lies beyond the end of the history and is not computed (menu "Edit", then "To end of timeline")
tree-fold-close = Click to collapse the folder.
tree-fold-open = Click to expand the folder.
tree-header = Feature tree
tree-body-hint = Body: click selects it for the properties and the history bar; double-click opens "Edit"; right-click shows more commands.
tree-show-body = Show body
tree-hide-body = Hide body (in the view only)
tree-in-progress = { $name } (being edited)
tree-sketch-hint = Double-click opens the sketch for editing; right-click shows more commands.
tree-edit = Edit
tree-rename = Rename
tree-activate = Activate
tree-suppress = Suppress
tree-extrude = Extrude …
tree-revolve = Revolve …
tree-delete = Delete
tree-plane-hint = Base plane; "Create", then "Sketch" creates a sketch on it.
tree-apply = Apply
tree-confirm-title = Confirm

## File chooser (window)
chooser-replace-hint = This file already exists. Click again to replace it.
chooser-choose-folder = Choose this folder
chooser-no-subfolders = There are no subfolders here.
chooser-chosen-prefix = Chosen:
chooser-no-folder = Please choose an existing folder.
chooser-no-matching-files = There are no matching files here.
chooser-open-hint = Double-click to open
chooser-folder-missing = This folder does not exist.
chooser-bad-file-name = Please enter a valid file name (without special characters such as : ? * or slashes).
chooser-pick-existing-file = Please choose an existing file.
chooser-folder-up = Up one folder
chooser-truncated = Only the first { $n } entries are shown.
chooser-file-name = File name
chooser-replace = Replace
chooser-open = Open

## Formula fields
field-fx-label = Convert to value
field-fx-hint = Formula. Click to insert the calculated value and remove the formula.
field-empty = Please enter a number or a formula.
field-names = Names: { $list }
field-error = { $label }: { $error }

## Create bodies
create-box-title = Create box
create-cylinder-title = Create cylinder
create-sphere-title = Create sphere
create-cone-title = Create cone
create-extrude-title = Extrude sketch
create-revolve-title = Revolve sketch
create-field-width = Width (x)
create-field-depth = Depth (y)
create-field-height-z = Height (z)
create-field-radius = Radius
create-field-height = Height
create-axis-construction = Construction line { $n } ({ $length })
create-axis-line = Line { $n } ({ $length })
create-none = (none)
create-choose = (please choose)
create-sketch = Sketch
create-profile = Profile
create-axis = Axis of revolution
create-sketch-missing = Sketch not found or not computed.
create-no-profile = The sketch has no closed profile.
create-profile-label = Profile { $n }: { $area } mm²
create-profile-label-holes = Profile { $n }: { $area } mm², { $holes ->
    [one] 1 hole
   *[other] { $holes } holes
}
create-rejected = Not usable: { $reason }
create-no-axis = The sketch has no line that can serve as the axis of revolution.
create-create = Create
create-pick-axis = Please choose an axis of revolution (a line of the sketch).
create-axis-gone = The chosen axis of revolution no longer exists in this sketch; please choose again.

## Fillet and chamfer
blend-fillet-title = Fillet
blend-chamfer-title = Chamfer
blend-no-edge = Please click an edge in the view first.
blend-art-round = Fillet
blend-art-variable = Fillet with variable radius
blend-art-chamfer-equal = Chamfer: equal distance
blend-art-chamfer-two = Chamfer: two distances
blend-art-chamfer-angle = Chamfer: distance and angle
blend-field-radius = Radius
blend-field-radius-n = Radius { $n }
blend-field-position-n = Position { $n }
blend-field-distance = Distance
blend-field-distance-1 = Distance 1
blend-field-distance-2 = Distance 2
blend-field-angle = Angle
blend-too-many-points = A variable radius can have at most { $max } points
blend-too-small = "{ $label }" is too small; the smallest possible size is { $min } mm
blend-size-invalid = "{ $label }" must be a number greater than zero (entered: "{ $text }")
blend-angle-invalid = "Angle" must be between { $min }° and { $max }° (entered: "{ $text }")
blend-position-range = "Position { $n }" must be between 0 and 100 %
blend-body-n = Body { $n }
blend-mixed-bodies = The edges belong to different bodies ({ $names }); only one body at a time
blend-chosen-edges = Chosen edges: { $n }
blend-click-edge = Click an edge in the view (Shift: several).
blend-point-n = Point { $n }
blend-remove = Remove
blend-add-point = Add point
blend-face-1 = Face 1
blend-min-size = Smallest size: { $min } mm
blend-chain = Tangent chain (include smoothly connected edges)

## Edit feature
edit-title = Edit "{ $name }"
edit-stale = The feature has been changed in the meantime; the window now shows the current state. Please check the values and apply again.
edit-numbers-only = { $label }: Only numbers are possible here.
edit-invalid-number = Invalid number for "{ $label }": "{ $text }"
edit-too-big = The value for "{ $label }" is too large (absolute value at most { $max } mm)
edit-op-difference = Subtract (target − tool)
edit-rejected = The change was rejected: { $error }
edit-remove-point = Remove point
edit-target = Target
edit-tool = Tool
edit-body = Body: { $name }
edit-edges = Edges: { $n }
edit-sketch-not-computed = The sketch has not been computed.

## Boolean operation
boolean-title = Boolean operation
boolean-body-a = Body A
boolean-body-b = Body B
boolean-subtract = Subtract (A − B)
boolean-run = Run
boolean-same-body = Body A and body B must be different.

## Parameters
param-title = Parameters
param-add-row = Add row
param-delete-row = Delete row
param-replace = Replace with value
param-empty-table = No parameters yet.
param-dialog-closed = The window was closed because parameters changed.
param-col-name = Name
param-col-formula = Formula
param-col-value = Value
param-col-kind = Type
param-col-comment = Comment
param-gone = The parameter no longer exists.
param-live-more = { $first } (+ { $n ->
    [one] 1 more error
   *[other] { $n } more errors
})
param-field-label = { $column } of { $name }
param-other-row = Parameter "{ $name }": { $message }

## Properties
props-name = Name
props-kind = Type
props-status = Status
props-status-ok = computed
props-status-error = Error: { $error }
props-status-skipped = skipped: { $why }
props-status-not-computed = not computed (behind the timeline marker)
props-open-sketch = Open sketch
props-edit = Edit …
props-nothing-selected = Nothing selected.
props-volume = Volume
props-surface = Surface area
props-centroid = Center of mass
props-dimensions = Dimensions
props-faces-edges = Faces / edges
props-faces = Faces
props-faces-in-file = Faces in the file
props-accuracy = Accuracy of the file
props-accuracy-unknown = unknown (assumed { $assumed })
props-step-coarse-hint = Calculating with this body may fail.
props-position-label = Position x / y / z in mm
props-rotation-label = Rotation x / y / z in degrees
props-position-x = Position x
props-position-y = Position y
props-position-z = Position z
props-selected-face = Selected face: { $kind }
props-selected-edge = Selected edge: { $kind }, length ≈ { $length } mm
props-click-face-or-edge = Click a face or an edge in the viewport.
props-mesh-body = Mesh body
props-triangles-label = Triangles
props-closed-at-import = Closed at import
props-yes = yes
props-no = no
props-at-import = At import: { $report }
props-mesh-body-hint = Mesh bodies can be moved, rotated, scaled, measured and exported, but not combined with other bodies, filleted or used as a face for a sketch.
props-on-face = Face "{ $role }" of "{ $name }"
props-plane = Plane
props-curves = Curves
props-edge-copies = Edge copies
props-size = Size
props-position = Position
props-radius = Radius
props-height = Height
props-sketch = Sketch
props-profile = Profile
props-axis = Axis of revolution
props-curve-n = Curve { $n }
props-operation = Operation
props-body = Body
props-displacement = Displacement
props-edges = Edges
props-points = Points
props-tangent-chain = Tangent chain
props-on = on
props-off = off
props-mesh = Mesh
props-triangles = { $n ->
    [one] 1 triangle
   *[other] { $n } triangles
}
props-rotation = Rotation
props-scale = Scale

## Mesh bodies
mesh-preview-note = Note: The mesh is very large; the view shows only some of the triangles. It is saved and exported in full.
mesh-open-edges = { $n ->
    [one] 1 open edge
   *[other] { $n } open edges
}
mesh-flipped-edges = { $n ->
    [one] 1 edge with flipped triangles
   *[other] { $n } edges with flipped triangles
}
mesh-non-manifold-edges = { $n ->
    [one] 1 edge shared by more than two triangles
   *[other] { $n } edges shared by more than two triangles
}
mesh-closed = The mesh is closed.
mesh-not-closed = The mesh is not closed.
mesh-not-fully-closed = The mesh is not fully closed: { $parts }.
mesh-degenerate = { $n ->
    [one] 1 triangle has no area.
   *[other] { $n } triangles have no area.
}

## Import (mesh and STEP)
import-reading = Reading the file …
import-stale = The document has changed since the file was read. Please choose the file again.
import-lost = The calculation ended unexpectedly.
import-choose-file = Please choose a file first.
import-unit-title = Unit of the file
import-unit-mm = Millimeters
import-unit-cm = Centimeters
import-unit-m = Meters
import-unit-inch = Inches
import-unit-inch-short = in
import-import = Import
meshimport-title = Import mesh
meshimport-choose = Choose file (STL or OBJ)
meshimport-check = Check
meshimport-file = File: { $name }
meshimport-counts = { $triangles } triangles, { $points } points
meshimport-open-hint = You can still create and show the mesh body; volume and surface area are then only approximate.

## Import STEP
menu-file-import-step = Import STEP …
stepimport-title = Import STEP
stepimport-checking = Checking the bodies …
stepimport-no-unit = The file does not name a length unit; assumed: millimeters.
stepimport-unclear-unit = The file names different length units. Please choose the unit of the file.
stepimport-unreadable-unit = The unit of the file cannot be read. Please choose the unit of the file.
stepimport-mixed-unit-hint = The bodies in the file use different length units; each one is read in its own unit.
stepimport-free-skipped = A body without its own unit was skipped because the file names different length units. A unit you choose applies to all bodies.
stepimport-mixed-unit = mixed
stepimport-nothing-checked = Please tick at least one body.
stepimport-size-hint = Size of the bounding box, rounded to 0.1 mm; approximate for curved surfaces.
stepimport-hint-mixed-chosen-all = The file names different length units; chosen: { $unit } for all bodies.
stepimport-hint-named-chosen = The file names { $own }; chosen: { $unit }.
stepimport-hint-missing-chosen = The file does not name a length unit; chosen: { $unit }.
stepimport-hint-mixed-chosen = The file names different length units; chosen: { $unit }.
stepimport-hint-unreadable-chosen = The unit of the file cannot be read; chosen: { $unit }.
stepimport-about = about { $size }
stepimport-summary = { $n ->
    [one] 1 body imported
   *[other] { $n } bodies imported
}
stepimport-and-more = and { $n } more
stepimport-summary-skipped = { $head }, { $n } skipped: { $list }
stepimport-status-skipped = will be skipped: { $why }
stepimport-status-ok = will be imported
stepimport-status-ok-warning = will be imported – { $warnings }
stepimport-wait = Please wait until the check is finished.
stepimport-choose = Choose file (STEP)
stepimport-other-file = Choose another file …
stepimport-file = File
stepimport-col-unit = Unit
stepimport-usable = { $total ->
    [one] { $usable } of { $total } body can be imported.
   *[other] { $usable } of { $total } bodies can be imported.
}

## Bodies, sketches and model messages
state-undo-failed = Undo is not possible.
state-redo-failed = Redo is not possible.
state-sketch-open-lock = Please finish the sketch first.
state-mesh-note = Note: The mesh of this body is inexact in places; the view may show small gaps.
state-sketch-label = Sketch
state-mesh-label = Mesh body
state-unnamed = Untitled
state-feature-n = Feature { $n }
state-create-sizes-positive = Creating failed: The sizes must be greater than zero
state-create-sizes-max = Creating failed: The sizes may be at most { $max } mm
state-pick-two-bodies = Please choose two different bodies
state-pick-edge-first = Please click an edge first
state-body-not-found = Body not found
state-face-not-found = Face not found
state-sketch-not-found = Sketch not found
state-profile-not-found = Profile not found
state-invalid-move = Invalid displacement
state-body-invisible = The body is no longer visible after finishing the sketch
state-move-rejected = Move rejected: The body would lie outside ±{ $max } mm
state-move-label = Move
state-move-gone = The move no longer exists
state-move-discarded = Move discarded: The feature "{ $name }" could no longer be computed afterwards ({ $message })
state-end-sketch-first = Please finish the sketch first (button "Finish sketch" at the bottom), then try again
state-delete-blocked = "{ $name }" cannot be deleted because these features depend on it: { $list }
state-tessellation-failed = Tessellation failed: { $error }
state-body-note = "{ $name }": { $note }
state-sketch-planar-only = Sketches can only be created on flat faces
state-sketch-on-face-failed = A sketch is not possible on this face: { $error }
state-sketch-commit-failed = The sketch could not be applied: { $error }
state-no-open-sketch = No sketch is open
state-edge-body-later = This body only exists after the sketch (for example through a later move or a later Boolean operation); its edges cannot be copied into it
state-edge-copy-failed = This edge cannot be copied: { $error }
state-sketch-not-computed = The sketch "{ $name }" is not computed ({ $reason })
state-height-positive = The height must be greater than zero
state-extrusion-name = Extrusion: { $name }
state-revolve-name = Revolve: { $name }
state-feature-failed = The feature could not be computed
state-mesh-locked = Mesh bodies cannot be edited.
state-need-two-bodies = This needs at least two bodies.
state-need-one-body = This needs at least one body.
state-pick-planar-face = First click a flat face of a body.
state-not-mesh = This is not a mesh body
state-field-no-formula = This field cannot have a formula

## Skizzen
sketch-constraint-coincident = Coincident
sketch-constraint-point-on-curve = Point on curve
sketch-constraint-horizontal = Horizontal
sketch-constraint-vertical = Vertical
sketch-constraint-parallel = Parallel
sketch-constraint-perpendicular = Perpendicular
sketch-constraint-tangent = Tangent
sketch-constraint-equal = Equal
sketch-constraint-fixed = Fixed
sketch-constraint-symmetric = Symmetric
sketch-constraint-midpoint = Midpoint
sketch-constraint-distance = Distance
sketch-constraint-distance-point-line = Point–line distance
sketch-constraint-radius = Radius
sketch-constraint-diameter = Diameter
sketch-constraint-angle = Angle
sketch-constraint-arc-radius = Arc radius
sketch-constraint-ellipse-major = Major semi-axis
sketch-constraint-ellipse-minor = Minor semi-axis
sketch-constraint-ellipse-angle = Ellipse angle
sketch-overlay-midpoint = Mid
sketch-overlay-symmetric = Sym
sketch-overlay-on-curve = on
sketch-overlay-perpendicular = perp
sketch-driven-no-formula = A driven dimension does not constrain anything and cannot have a formula.
sketch-formula-conflict = With the formula values, constraints contradict each other
sketch-formula-unsolvable = The sketch cannot be solved with the formula values
sketch-still-conflicting = The sketch still contains contradictions
sketch-still-unsolved = The sketch still cannot be fully solved
sketch-change-conflict = Contradiction: the change conflicts with existing constraints
sketch-change-unsolved = The sketch could not be solved after the change
sketch-invalid-position = Invalid position
sketch-radius-positive = The radius must be greater than zero
sketch-arc-start-center = The start point must not be at the center
sketch-arc-end-center = The arc end must not be at the center
sketch-arc-end-on-start = The end is on the start; the arc would have no length
sketch-major-axis-positive = The major semi-axis must be longer than zero
sketch-minor-axis-positive = The minor semi-axis must be greater than zero
sketch-spline-max = A spline has at most { $max } points; please finish it
sketch-spline-point-twice = This point is already part of the spline
sketch-spline-min = A spline needs at least 2 points
sketch-drag-blocked = The point cannot be dragged there because of the constraints
sketch-nothing-selected = Nothing is selected
sketch-action-mismatch = "{ $action }" does not fit the current selection
sketch-selection-invalid = The selection is invalid
sketch-length-lines-only = Length only exists for lines
sketch-cannot-measure = The dimension cannot be measured
sketch-apply-conflict = Contradiction: "{ $action }" conflicts with existing constraints
sketch-apply-unsolved = "{ $action }" could not be solved
sketch-became-driven = This dimension is already determined by other constraints and is only displayed
sketch-redundant = This constraint is redundant
sketch-constraint-missing = Constraint not found
sketch-not-dimension = This is not a dimension
sketch-only-dimension-driven = Only dimensions can be driven
sketch-stays-driven = This dimension is already determined by other constraints and stays driven
sketch-enter-number = Please enter a number
sketch-pick-edge = Please click a body edge
sketch-status-solved = Fully constrained
sketch-status-dof-one = 1 degree of freedom
sketch-status-dof = { $n } degrees of freedom
sketch-status-redundant = , { $n } redundant
sketch-status-conflict = Contradiction between { $n } constraints
sketch-status-unsolvable = Cannot be solved
sketch-tool-select = Select
sketch-tool-select-hint = Click selects, Shift + click adds to the selection; points can be dragged.
sketch-tool-point = Point
sketch-tool-point-hint = Click places a point.
sketch-tool-line-hint = Click points one after another; Enter, double-click or Esc ends the polyline.
sketch-tool-rectangle-hint = Click two opposite corners.
sketch-tool-circle-hint = Click the center, then a point on the circle.
sketch-tool-arc = Arc
sketch-tool-arc-hint = Click center, start and end (counterclockwise).
sketch-tool-ellipse = Ellipse
sketch-tool-ellipse-hint = Click the center, the end of the major semi-axis, then the width.
sketch-tool-spline = Spline
sketch-tool-spline-hint = Click control points; Enter or double-click creates the spline.
sketch-tool-project-edge-hint = Click an edge of a body; it is copied as a fixed construction line.
sketch-section-constraints = Constraints
sketch-not-fitting = Does not fit the current selection.
sketch-section-edit = Edit
sketch-select-curves-first = Select curves first.
sketch-mirror-button = Mirror …
sketch-mirror-disabled = Select curves, and last the line to mirror about.
sketch-pattern-button = Pattern …
sketch-delete-disabled = Nothing is selected.
sketch-auto-axis = Detect horizontal/vertical
sketch-auto-axis-hint = Nearly horizontal or vertical lines automatically get the matching constraint while you draw.
sketch-section-list = Sketch constraints
sketch-list-empty = No constraints yet.
sketch-driven = driven
sketch-driven-hint = Driven dimension (does not constrain anything)
sketch-delete-row = Delete constraint
sketch-fx-label = Edit formula
sketch-fx-hint = This dimension follows a formula. Click to open the dimension field.
sketch-end = Finish sketch
sketch-formula-has-error = The formula contains an error.
sketch-mirror-axis = Axis: the last selected line.
sketch-mirror-do = Mirror
sketch-offset-x = Offset x
sketch-offset-y = Offset y
sketch-pattern-count = Number of copies

## Drawings
drawing-lost = The calculation ended unexpectedly.
drawing-no-bodies = There is no body that can be drawn.
drawing-mesh-bodies-only = All bodies are meshes; a drawing needs exact bodies.
drawing-meshes-skipped = Mesh bodies are not drawn: { $names }.
drawing-sketch-blocked = Finish the sketch first.
drawing-add-read-only = Nothing can be added in an old version.
drawing-read-only = Nothing can be changed in an old version.
drawing-name-empty = Please enter a name.
drawing-name-duplicate = A drawing with this name already exists.
drawing-projection-first = First angle (Europe)
drawing-projection-third = Third angle (USA)
drawing-suggested-name = Drawing { $n }
drawing-missing = The drawing is missing.
drawing-new-title = New drawing
drawing-new-sheet = Sheet
drawing-new-name = Name
drawing-new-paper = Paper
drawing-new-landscape = Landscape
drawing-new-views = Views
drawing-new-base-view = Base view
drawing-scale = Scale
drawing-menu-new = New drawing …
drawing-menu-open = Open: { $name }
drawing-menu-to-model = To 3D model
drawing-empty = No drawing yet. Use "Drawing" in the menu to create one.
drawing-gone-view = The drawing or view no longer exists.
drawing-follow-page-delete = A continuation sheet of the parts list cannot be deleted on its own. It disappears by itself when the rows fit on fewer sheets.
drawing-parts-sheet-rename = Parts list sheets cannot be renamed.
drawing-detail = { $paper } { $orientation }, { $projection }
drawing-landscape = landscape
drawing-portrait = portrait
drawing-angle-first = First angle
drawing-angle-third = Third angle
drawing-del-parts = Delete the parts list?
drawing-del-parts-pages = Delete the parts list with { $n } sheets?
drawing-del-ask = { $views ->
    [0] Delete drawing "{ $name }"?
    [one] Delete drawing "{ $name }" with 1 view?
   *[other] Delete drawing "{ $name }" with { $views } views?
}
drawing-del-section-line = the section line in "{ $source }"
drawing-del-derived-views = the derived views ({ $names })
drawing-del-dims = { $n ->
    [one] 1 dimension
   *[other] { $n } dimensions
}
drawing-del-list-1 = { $a }
drawing-del-list-2 = { $a } and { $b }
drawing-del-list-3 = { $a }, { $b } and { $c }
drawing-del-section = { $count ->
    [one] Section view "{ $name }" will be deleted. With it, { $list } disappears.
   *[other] Section view "{ $name }" will be deleted. With it, { $list } disappear.
}
drawing-del-view = View "{ $name }" will be deleted.
drawing-del-view-dims = View "{ $name }" will be deleted, and { $dims }.
drawing-del-view-derived = View "{ $name }" has derived views: { $names }. They will be deleted too.
drawing-del-view-derived-dims = View "{ $name }" has derived views: { $names }. They will be deleted too, and { $dims }.
drawing-del-dims-along = { $n ->
    [one] 1 dimension will be deleted too
   *[other] { $n } dimensions will be deleted too
}
drawing-del-changed = The consequences of deleting have changed; please check
drawing-row-tip = Double-click opens the drawing; right-click shows more commands.
drawing-view-row-tip = Click opens the drawing and selects the view.
drawing-hide-hidden = Hide hidden edges
drawing-show-hidden = Show hidden edges
drawing-fit-sheet = Fit sheet
drawing-hint = Clicking selects a view or a dimension, dragging moves it. The mouse wheel zooms; the middle mouse button or Shift + drag pans the sheet.
drawing-centerlines = Centerlines
drawing-no-iso-centerlines = Isometric views have no centerlines; they only appear in the orthographic views (front, top, side).
drawing-no-view-selected = Select a view.
drawing-suggest = Suggest dimensions
drawing-lost-tip = The dimension has lost its reference.
drawing-parent-missing = The source view cannot be drawn.
drawing-none-open = No drawing is open.
drawing-no-views-on-parts-sheet = There are no views on the parts list sheet.
drawing-view-front = Front
drawing-view-back = Back
drawing-view-left = Left
drawing-view-right = Right
drawing-view-top = Top
drawing-view-bottom = Bottom
drawing-view-isometric = Isometric
drawing-view-section = Section { $letter }–{ $letter }
drawing-view-numbered = { $name } { $n }
drawing-to-model = 3D model
drawing-current = Drawing: { $name }
drawing-add-view = Add view
drawing-zoom-out = Zoom out
drawing-zoom-in = Zoom in
drawing-suggest-tip-view = Places the most important dimensions in the selected view.
drawing-suggest-tip-all = Places the most important dimensions in all views.
drawing-view-header = View
drawing-view-title = View "{ $name }"
drawing-view-delete = Delete view
drawing-section-position = Section position (mm)
drawing-section-flip = Reverse viewing direction
drawing-section-hatch = Hatching
drawing-section-hatch-spacing = Hatch spacing (mm)
drawing-quoted = "{ $name }"
drawing-views-failed = { $count ->
    [one] The view { $names } could not be calculated. { $reason }
   *[other] The views { $names } could not be calculated. { $reason }
}
drawing-views-failed-no-dims = { $count ->
    [one] The view { $names } could not be calculated and got no dimensions.
   *[other] The views { $names } could not be calculated and got no dimensions.
}
drawing-pending = calculating … { $percent }%

## Dimensions
dim-views-pending = The views are still being calculated.
dim-no-drawing = The drawing does not exist.
dim-no-view = The view does not exist.
dim-no-isometric = There are no dimensions in the isometric view.
dim-no-normal-view = The drawing has no view yet in which dimensions can be suggested.
dim-not-bound = A suggested dimension could not be attached to the model reliably and was skipped.
dim-full = The drawing already has the maximum of { $max } dimensions.
dim-added = { $n ->
    [0] No dimension was added.
    [one] 1 dimension was added.
   *[other] { $n } dimensions were added.
}
dim-none-new = There are no new dimensions to suggest.
dim-note-lost = { $n ->
    [one] 1 dimension has lost its reference.
   *[other] { $n } dimensions have lost their reference.
}
dim-note-outside = { $n ->
    [one] 1 dimension is outside the sheet.
   *[other] { $n } dimensions are outside the sheet.
}
dim-rest-not-set = { $n ->
    [one] 1 more dimension was not placed.
   *[other] { $n } more dimensions were not placed.
}
dim-skipped = A dimension was skipped: { $reason }
dim-set = { $n ->
    [one] 1 dimension was placed.
   *[other] { $n } dimensions were placed.
}
dim-not-dimensionable = This reference cannot be dimensioned.
dim-tool-no-isometric = There are no dimensions in an isometric view.
dim-angle-lines = An angle dimension needs two straight edges.
dim-silhouette-note = The measurement goes to the center of the cylinder.
dim-no-views-tip = Create a view first.
dim-missed = Please click a line of the view.
dim-tool = Dimension
dim-tool-tip = Place dimensions by hand: click lines or corners, then click to place the dimension line.
dim-read-only-tip = Dimensions cannot be placed in an old version.
dim-isometric-tip = There are no dimensions in an isometric view. Add a front, side or top view.
dim-kind-horizontal = Horizontal
dim-kind-vertical = Vertical
dim-kind-aligned = Aligned
dim-kind-diameter = Diameter
dim-kind-radius = Radius
dim-kind-angle = Angle
dim-mode-off = Off
dim-mode-chain = Chain
dim-mode-baseline = Baseline
dim-incomplete = The dimensions are incomplete: only { $set } of { $total } were placed.
dim-not-enough-lines = Not enough lines are selected yet.
dim-pick-mode-first = Please choose a dimension type first.
dim-no-group = The drawing has no free group number left.
dim-hint-round = Click a circle, an arc or the edge of a cylinder.
dim-hint-angle-first = Click the first straight edge.
dim-hint-angle-second = Click the second straight edge.
dim-hint-linear-first = Click the first edge or corner.
dim-hint-linear-second = Click the second edge or corner.
dim-hint-chain-first = Click the first point of the chain.
dim-hint-chain-next = Click the next point of the chain.
dim-hint-baseline-first = Click the baseline edge.
dim-hint-baseline-target = Click the first target.
dim-hint-more = Click more points; a click on empty space, a double-click or Esc places the dimensions.
dim-hint-place = Click to place the dimension line. Esc cancels.
dim-gone = The dimension no longer exists.
dim-placeholder-hint = <> stands for the calculated value
dim-lost-prefix = The dimension has lost its reference
dim-tol-none = None
dim-tol-symmetric = Symmetric ±
dim-tol-limits = Upper and lower
dim-tol-limit-sizes = Limit sizes
dim-tol-fit = Fit
dim-err-enter-number = Please enter a number.
dim-err-not-a-number = "{ $text }" is not a number.
dim-err-too-big = The number must not exceed { $max }.
dim-deviation = Deviation
dim-err-negative = Deviation: The deviation must not be negative.
dim-upper = Upper
dim-lower = Lower
dim-upper-deviation = Upper deviation
dim-lower-deviation = Lower deviation
dim-err-limits-order = { $label }: The upper deviation must not be smaller than the lower one.
dim-err-fit-empty = Fit: Please enter a fit.
dim-err-fit-long = Fit: The text may have at most { $max } characters.
dim-err-text-long = Dimension text: The text may have at most { $max } characters.
dim-err-decimals = Decimal places: At most { $max } are possible.
dim-edit-title = Edit dimension
dim-card-measure = Dimension
dim-card-tolerance = Tolerance
dim-kind = Type
dim-value = Value
dim-decimals = Decimal places
dim-text = Dimension text
dim-tolerance-kind = Tolerance type
dim-deviation-pm = Deviation ±
dim-limit-sizes-note = The maximum and minimum size are shown.
dim-delete = Delete dimension
dim-empty = No dimensions yet. Use the "Dimension" tool or "Suggest dimensions" to create some.
dim-chain-ask = This dimension belongs to a chain; the other dimensions stay
dim-no-result = The dimension cannot be calculated.
dim-section = Dimensions
dim-row-tip = Click selects the dimension; double-click opens "Edit dimension"; right-click shows more commands.
dim-view = View

## Balloons
balloon-tool = Balloon
balloon-tool-tip = Click a line of a part to place a balloon number. You can move the circle afterwards.
balloon-hint = Click a line of a part. Esc ends the tool.
balloon-missed = Please click a line of a part.
balloon-read-only-tip = Balloon numbers cannot be placed in an old version.
balloon-parts-sheet-tip = There are no balloon numbers on the parts list sheet.
balloon-lost-tip = The part for this balloon number no longer exists.
balloon-too-many = A drawing can have at most { $max } balloon numbers.
balloon-lost-note = { $n ->
    [one] 1 balloon number has lost its part.
   *[other] { $n } balloon numbers have lost their part.
}
balloon-empty = No balloon numbers yet. The "Balloon" tool places them.
balloon-section = Balloons
balloon-row-tip = Click selects the balloon number; right-click shows more commands.
balloon-row = Balloon { $n } – { $name }
balloon-row-lost = Balloon ? – part missing

## Sections
section-tool = Section
section-tool-tip = Two clicks in an orthographic view define the section line. This creates a section view.
section-hint-first = Click the start of the section line in an orthographic view. Esc ends the tool.
section-hint-second = Click the end of the section line. The line snaps to horizontal or vertical.
section-not-from-here = A section can only be made from an orthographic view.
section-too-short = Please click two different points.
section-full = A sheet can have at most { $max } sections.
section-nudged = The section plane lay exactly on a face or edge and was moved by 0.001 mm.
section-read-only-tip = Sections cannot be created in an old version.
section-parts-sheet-tip = There are no sections on the parts list sheet.
section-no-straight-tip = A section needs an orthographic view on the sheet.
section-scale-locked-tip = The scale follows the source view.
section-not-a-number = Please enter a number.

## Sheet info and parts list
sheet-title = Sheet info
sheet-button = Sheet info
sheet-button-tip = Edit the title, drawing number, revisions and other title block entries.
sheet-menu-entry = Sheet info …
sheet-no-drawing-open = Open a drawing first.
sheet-owner-missing = Please enter the owner. It belongs on every drawing.
sheet-cut-warning = Does not fit the field completely and is shortened on the sheet.
sheet-revisions-on-sheet = The sheet shows the last 3 revisions.
sheet-gone = The drawing no longer exists.
sheet-notes-full = At most { $max } note lines are possible.
sheet-field-owner = Owner
sheet-field-number = Drawing number
sheet-field-title = Title
sheet-field-subtitle = Subtitle
sheet-field-doc-type = Document type
sheet-field-status = Document status
sheet-field-issued = Issue date
sheet-field-revision = Revision
sheet-field-sheet-of = Sheet
sheet-field-format = Format
sheet-field-scale = Scale
sheet-field-creator = Created by
sheet-field-checked = Checked by
sheet-field-approved = Approved by
sheet-field-material = Material
sheet-field-weight = Weight
sheet-field-language = Language
sheet-field-tolerance = General tolerance
sheet-section-project = Project
sheet-section-sheet = This sheet
sheet-section-revisions = Revisions
sheet-notes-all = Notes for all sheets
sheet-notes-reset = Reset notes
sheet-notes-own = Own notes for this sheet
sheet-no-revisions = No revisions yet.
sheet-rev-letter = Letter
sheet-rev-description = Description
sheet-rev-date = Date
sheet-rev-name = Name
sheet-rev-add = Add revision
sheet-line-delete = Delete line
sheet-line-add = Add line
sheet-parts-error = Parts list: { $reason }
sheet-parts-too-many-overrides = The parts list can have at most { $max } custom row texts
sheet-too-many-revisions = At most { $max } revisions are possible.
parts-section = Parts list
parts-switch = Parts list on this sheet
parts-too-many-on-sheet = More rows do not fit on the sheet. Use the parts list sheet for them.
parts-too-many-rows = More than { $max } rows do not fit on the parts list sheets.
parts-none = There are no parts yet.
parts-create = Create parts list sheet
parts-open = Open parts list sheet
parts-col-name = Part name
parts-col-qty = Quantity
parts-col-material = Material
parts-col-note = Remark

## Export
export-title = Export
export-format = Format
export-quality = Quality
export-quality-coarse = Coarse (up to 0.1 mm deviation)
export-quality-normal = Normal (up to 0.01 mm)
export-quality-fine = Fine (up to 0.001 mm)
export-quality-hint = Finer means more triangles and larger files.
export-scope = Scope
export-scope-selected = Selected body
export-scope-selected-tip = Select a body first.
export-scope-visible = All visible bodies
export-scope-note = Suppressed steps are not included.
export-format-stl = STL – for 3D printing
export-format-obj = OBJ – for many 3D programs
export-format-3mf = 3MF – for 3D printing, with names
export-format-glb = glTF (.glb) – for viewing and showing
export-format-step = STEP – for other CAD programs (exact)
export-format-svg = Drawing as SVG
export-format-dxf = Drawing as DXF
export-format-pdf = Drawing as PDF
export-step-exact = Fillets and holes stay exact.
export-drawing-card = Drawing
export-no-drawings = No drawing available.
export-pdf-selected = Selected sheet
export-pdf-all = All sheets
export-pdf-all-hint = With "All sheets", all drawings go into the PDF one after another.
export-frame-switch = Include sheet frame
export-sheets-count = Sheets: { $n }
export-views-count = Views: { $n }
export-sheet-line = Sheet: { $sheet }
export-dims-count = Dimensions: { $n }
export-dims-outside = Dimensions outside the sheet: { $n }
export-centerlines-line = Centerlines: { $value }
export-centerlines-yes = yes
export-centerlines-no = no
export-centerlines-partly = partly
export-lines-computing = Lines: calculating …
export-lines-not-computed = Lines: not calculated
export-lines-counts = Lines: { $visible } visible, { $hidden } hidden
export-recompute = Recalculate
export-triangles = { $triangles } triangles, { $size }
export-bodies-faces = Bodies: { $bodies }, faces: { $faces }
export-ready-all = All sheets are calculated.
export-ready-list = The parts list is calculated.
export-ready-views = All views are calculated.
export-computing-list = Calculating the parts list …
export-computing-views = Calculating the views …
export-computing-step = Checking the bodies …
export-computing-mesh = Calculating the mesh …
export-sheet-progress = Calculating sheet { $n } of { $total } …
export-writing = Writing the file …
export-done = Done: "{ $name }" written ({ $size }).
export-failed = Not written: { $reason }
export-save-as = Save as
export-start = Export …
export-wait-hint = Possible once the calculation is finished.
export-size-under = under 0.1 MB
export-size-estimate = approx. { $size }
export-cancelled = Cancelled
export-lost = The calculation ended unexpectedly.
export-not-closed-warning = The file is still written. Some 3D printing programs report such meshes as faulty.
export-writing-hint = Writing is still in progress. Please wait or cancel first.
export-step-nothing = There is no body that can be written as a STEP file.
export-step-nothing-written = There is no body that can be written as a STEP file
export-no-drawing = There is no drawing yet. You can create one under "Drawing" in the menu.
export-no-view = The drawing "{ $name }" has no view yet.
export-stale = The selection has changed, please save again.
export-not-computed = Nothing has been calculated yet. Please wait or recalculate.
export-mesh-failed = "{ $name }" cannot be meshed: { $reason }
export-body-error = "{ $name }": { $reason }
export-view-failed = The view "{ $name }" cannot be calculated: { $reason }
export-views-not-placeable = The drawing contains views that cannot be placed on the sheet.
export-dim-lost = The dimension "{ $label }" in the view "{ $view }" has lost its reference: { $reason }. Please delete the dimension and place it again.
export-dim-changed = The dimension "{ $label }" in the view "{ $view }" has changed and can no longer be measured: { $reason }. Please delete the dimension and place it again.
export-lost-advice = Please delete the dimension and place it again.
export-balloon-lost = The balloon in the view "{ $view }" belongs to a part that no longer exists. Please delete the balloon and place it again.
export-outside = { $n ->
    [one] 1 dimension lies outside the sheet and is still written.
   *[other] { $n } dimensions lie outside the sheet and are still written.
}
export-dxf-dim-long = A dimension text in the view "{ $view }" is too long for DXF: "{ $text }". Please shorten the custom text.
export-dxf-frame-long = A text in the sheet frame is too long for DXF. Please shorten the entries.
export-dxf-char = The character U+{ $code } in the field "{ $field }" cannot be written to DXF. Please change it.
export-dxf-field-long = The text in the field "{ $field }" is too long for DXF. Please shorten it.
export-pdf-char = The character { $char } in the field "{ $field }" cannot be written to a PDF.
export-pdf-char-dim = The character { $char } in a dimension text of the view "{ $view }" cannot be written to a PDF.
export-field-revision = Revision { $letter }
export-field-note = Note { $n }
export-field-parts = Parts list, item { $pos }, { $head }
export-in-sheet = In sheet "{ $sheet }": { $msg }
export-too-many-pages = A PDF can hold at most { $max } sheets, the document has { $n }. Please export single sheets.
export-pages = { $n ->
    [one] 1 sheet
   *[other] { $n } sheets
}
export-drawing-skipped = Mesh bodies are not included in the drawing: { $names }.
export-skipped = Mesh bodies are not written as STEP: { $names }. Choose STL, OBJ, 3MF or glTF for them.
export-shortened = Name shortened (at most { $max } characters in the file): { $names }
export-default-drawing-name = Drawing
export-default-body-name = Body

## Sketch error messages
sketch-err-what-value = The value
sketch-err-value-finite = The value must be a finite number
sketch-err-point-self = A point cannot coincide with itself
sketch-err-only-lines = This only applies to lines
sketch-err-two-points = Two different points are needed
sketch-err-two-objects = Two different objects are needed
sketch-err-equal-kinds = Equal only works for two lines or two circles/arcs
sketch-err-symmetric = Symmetry needs two different points and a line as the axis
sketch-err-only-circles = This only applies to circles and arcs
sketch-err-only-arcs = This only applies to arcs
sketch-err-only-ellipses = This only applies to ellipses
sketch-err-distance-negative = The distance must not be negative
sketch-err-dimension-positive = The dimension must be greater than zero
sketch-err-hint-minor-smaller = first make the minor semi-axis smaller
sketch-err-hint-major-larger = first make the major semi-axis larger
sketch-err-minor-longer = The minor semi-axis must not be longer than the major one; { $hint }
sketch-err-internal-constraint = This constraint is created internally
sketch-err-data-tolerance = The sketch tolerance is invalid
sketch-err-data-frame = The sketch coordinate system is invalid
sketch-err-what-sketch-value = A value of the sketch
sketch-err-data-point-value = A point refers to a missing value
sketch-err-data-curve-point = A curve refers to a missing point
sketch-err-data-circle-value = A circle refers to a missing value
sketch-err-data-ellipse-value = An ellipse refers to a missing value
sketch-err-data-spline-count = A spline has an invalid number of points
sketch-err-data-curve-deleted = A curve refers to a deleted point
sketch-err-data-constraint-place = A constraint has an invalid position
sketch-err-data-constraint-value = A constraint refers to a missing value
sketch-err-data-constraint-object = A constraint refers to a missing object
sketch-err-data-constraint-number = A constraint has an invalid numeric value
sketch-err-data-constraint-deleted = A constraint refers to a deleted object
sketch-err-data-constraint-extra = A constraint is missing its extra variable
sketch-err-object-twice = An object is selected more than once
sketch-err-copy-kinds = Only lines, circles and arcs can be copied
sketch-err-corners-finite = The corners must be finite numbers
sketch-err-rect-size = Width and height of the rectangle must be greater than zero
sketch-err-mirror-line = The mirror axis must be a line
sketch-err-mirror-self = The mirror axis cannot be mirrored itself
sketch-err-mirror-zero = The mirror axis has zero length
sketch-err-copies-range = The number of copies must be between 1 and { $max }
sketch-err-offset-finite = The offset must consist of finite numbers
sketch-err-offset-zero = The offset must not be zero
sketch-err-what-radius = The radius
sketch-err-what-major = The major semi-axis
sketch-err-what-minor = The minor semi-axis
sketch-err-edge-type-changed = The edge has changed its type
sketch-err-edge-no-range = The edge has no parameter range
sketch-err-edge-perpendicular = The edge is perpendicular to the sketch plane and would only be a point
sketch-err-arc-not-parallel = Arcs that are not parallel to the sketch plane are not supported
sketch-err-ellipse-arc = Elliptical arcs are not supported, only full ellipses
sketch-err-freeform-edge = Freeform edges cannot be taken into the sketch yet
sketch-err-curve-perpendicular = The curve is perpendicular to the sketch plane and would only be a line segment
sketch-err-what-x = The x coordinate
sketch-err-what-y = The y coordinate
sketch-err-line-zero = The line would have zero length
sketch-err-arc-center = Start and end must not be at the center
sketch-err-minor-range = The minor semi-axis must be greater than zero and at most as long as the major one
sketch-err-spline-count = A spline needs 2 to { $max } points
sketch-err-point-twice = A point is selected more than once
sketch-err-spline-neighbours = Adjacent points of the spline coincide
sketch-err-arc-off-circle = An arc end is not on the arc circle; solve the sketch first
sketch-err-spline-off-node = A spline end does not meet the adjacent point of the loop
sketch-err-spline-two-points = A spline needs at least two points
sketch-err-loop-no-area = The loop does not enclose an area
sketch-err-arc-closed = An arc starts and ends at the same point
sketch-err-spline-closed-branch = A spline starts and ends at the same point where other curves meet
sketch-err-line-zero-length = A line has zero length
sketch-err-t-junction = T-junction
sketch-err-loops-in-group = several loops in one group
sketch-err-arc-no-angle = An arc has (almost) no opening angle
sketch-err-not-ellipse = This is not an ellipse
sketch-err-ellipse-major-small = The major semi-axis of the ellipse is too small (at least { $tol } mm)
sketch-err-ellipse-minor-small = The minor semi-axis of the ellipse is too small (at least { $tol } mm)
sketch-err-ellipse-invalid = The ellipse is invalid: { $detail }
sketch-err-spline-invalid = The spline is invalid: { $detail }
sketch-err-spline-reverse = The spline cannot be reversed
sketch-err-hole = a hole
sketch-err-axis-line = The axis of revolution must be a line
sketch-err-axis-zero = The axis of revolution has no length
sketch-err-profile-no-axis-edge = The profile has no edge on the axis of revolution
sketch-err-profile-both-sides = The profile lies on both sides of the axis of revolution
sketch-err-target-finite = The target must be a finite position
sketch-err-what-target-x = The x coordinate of the target
sketch-err-what-target-y = The y coordinate of the target
sketch-err-invalid-input = Invalid input: { $detail }
sketch-err-unknown-object = The object does not exist in this sketch
sketch-err-degenerate = Degenerate geometry: { $detail }
sketch-err-wrong-type = Wrong object type: { $detail }
sketch-err-open-loop = The profile is not closed
sketch-err-ambiguous-topology = The profile is ambiguous: { $detail }
sketch-err-self-intersecting = The profile crosses or touches itself or another profile
sketch-err-ambiguous-nesting = The nesting of the profiles is ambiguous
sketch-err-unsupported-solid = This profile contains { $what } and cannot become a solid yet
sketch-err-unsupported-projection = This edge cannot be copied into the sketch yet: { $detail }
sketch-err-kernel = The kernel rejected the profile: { $detail }
sketch-err-tangent-kinds = Tangent only works between a line and a circle/arc/ellipse/spline, or between two circles/arcs
sketch-err-not-finite = { $what } must be a finite number
sketch-err-too-large = { $what } is too large (absolute value at most { $max } mm)

## Math error messages
math-knot-outside = the knot { $u } is not inside the range ({ $a }, { $b })
math-knot-multiplicity = inserting the knot { $u } { $r } times (currently { $s } times) would give a multiplicity above the degree { $p }
math-sweep-finite = start angle and sweep angle must be finite
math-sweep-large = the sweep angle is larger than a full turn
math-sweep-small = the sweep angle is too small (at least { $min })
math-point-range = { $what } must be finite and at most { $max }
math-what-center = the center
math-what-axis-a = semi-axis a
math-what-axis-b = semi-axis b
math-what-each-point = every point
math-what-axis-point = the axis point
math-axes-degenerate = the semi-axes are zero or parallel
math-points-params-count = points and parameters differ in number
math-two-points = at least two points are needed
math-max-points = at most { $max } points
math-params-finite = parameters must be finite
math-params-increasing = parameters must be strictly increasing
math-direction = the direction must be finite and not zero
math-axis-direction = the axis direction must be finite and not zero
math-profile-on-axis = the profile curve lies entirely on the axis of revolution
math-control-too-many = too many control points (at most { $max })
math-knot-count = the number of knots does not fit the control points and degree
math-weights-count = weights and control points differ in number
math-weights-range = weights must be between { $min } and { $max }
math-coords-range = coordinates must be finite and at most { $max }
math-system-size = system of equations with { $n } unknowns (allowed: 1 to { $max })
math-system-shape = matrix and right-hand side do not fit the size of the system of equations
math-system-finite = system of equations contains non-finite numbers
math-system-singular = system of equations is singular
math-system-singular-numeric = system of equations is numerically singular (solution not finite)
math-bound-positive = the error bound must be finite and positive
math-points-range = points must be finite and at most { $max }
math-points-coincide = the points coincide (total length zero)
math-fit-failed = fitting failed: { $detail }
math-fit-distance = distance { $err } above the bound
math-fit-unreachable = error bound { $tol } cannot be reached with at most { $cap } control points ({ $why })
math-knots-few = too few knots
math-knots-nonfinite = non-finite number
math-knots-decrease = knots decrease
math-knots-clamped = not clamped (start and end need degree+1 equal knots)
math-domain-empty = the parameter range is empty
math-knots-too-equal = start or end has more than degree+1 equal knots
math-knots-magnitude = knots and parameter range must be at most { $max }
math-domain-small = the parameter range is too small (at least { $min })
math-knots-close = two different knots are too close together
math-knot-repeated = interior knot repeated too often
math-arc-radii = the arc ends have different distances from the center
math-section-angle-small = the opening angle of the cross-section is too small
math-section-angle-180 = the opening angle of the cross-section is too close to 180°
math-section-finite = the cross-section has non-finite coordinates
math-sections-mismatch = cross-sections, derivatives and parameters do not fit together
math-params-increasing2 = the parameters must be strictly increasing
math-section-zero = a cross-section has zero length
math-tangent-finite = a tangent is not finite
math-sections-static = the cross-sections do not move along (degenerate surface)
math-max-sections = at most { $max } cross-sections
math-control-per-direction = too many control points per direction (at most { $max })
math-control-total = too many control points in total (at most { $max })
math-control-expected = expected { $nu } × { $nv } = { $total } control points, got { $got }
math-patches-too-many = the decomposition would give { $count } Bézier patches (at most { $max })
math-err-degree = Invalid degree { $degree } (allowed: 1 to { $max })
math-err-knots = The knot vector is invalid: { $detail }
math-err-control = Invalid control points: { $detail }
math-err-weights = Invalid weights: { $detail }
math-err-fit = The curve fit failed: { $detail }
math-err-input = Invalid input: { $detail }
math-err-too-large = The calculation exceeds a size limit: { $detail }
math-radius-few-points = A variable radius needs at least two points
math-radius-positions = The positions must be finite and strictly increasing
math-radius-values = Every radius must be finite and greater than zero

## Feature tree error messages
feature-what-box-size = The size of the box
feature-what-box = The box
feature-what-radius-height = Radius or height
feature-what-body = The body
feature-what-radius = The radius
feature-what-sphere = The sphere
feature-what-extrude-height = The height of the extrusion
feature-what-move = The move distance
feature-no-edge = No edge is selected
feature-too-many-edges = Too many edges (at most { $max })
feature-radius-points = A variable radius needs 2 to { $max } points
feature-what-blend-size = A size of the fillet or chamfer
feature-what-mesh-position = The position of the mesh body
feature-mesh-scale-range = The scale must be between 0.000001 and 1,000,000
feature-mesh-rotation = The rotation of the mesh body is invalid
feature-what-mesh = The mesh body
feature-target-tool = Target and tool must be different bodies
feature-freeform-prefix = For bodies with freeform faces: { $cause }
feature-freeform-hint = For bodies with freeform faces: { $cause }; please change the position or size of a body slightly
feature-not-sketch = Feature "{ $name }" is not a sketch
feature-no-profile = Profile { $n } does not exist
feature-no-profile-rejected = Profile { $n } does not exist; rejected curve chains of the sketch: { $reasons }
feature-face-not-flat = The face for the sketch is not flat
feature-kind-fixed = The type of a feature cannot be changed
feature-name-empty = The name must not be empty
feature-mesh-ids-used = The mesh ids are used up
feature-mesh-not-created = The mesh body could not be created
feature-face-no-check-point = The face has no check point
feature-edge-no-check-point = The edge has no check point
feature-step-not-in-history = The step does not belong to the history of this body
feature-rollback-unknown = A rollback point refers to an unknown step (id { $id })
feature-rollback-twice = A rollback point occurs twice (id { $id })
feature-rollback-same-body = The rollback points "{ $a }" and "{ $b }" are in the history of the same body
feature-feature-ids-used = The feature ids are used up
feature-marker-max = The marker can be at most after feature { $n }
feature-dep-suppressed = Feature "{ $name }" is suppressed
feature-dep-error = depends on feature "{ $name }" (error)
feature-dep-skipped = depends on feature "{ $name }" (skipped)
feature-snap-id-counter = The id counter is invalid
feature-snap-marker = The rollback marker is after the last feature
feature-snap-param-counter = The parameter counter is invalid
feature-snap-param-id = A parameter has an invalid id
feature-snap-param-id-twice = A parameter id occurs twice
feature-snap-feature-id = A feature has an invalid id
feature-snap-feature-id-twice = A feature id occurs twice
feature-snap-feature-name = A feature has no name
feature-snap-no-formula = Feature "{ $name }": This field cannot have a formula
feature-snap-mesh-id = A mesh has an invalid id
feature-snap-step-id = A STEP shape has an invalid id
feature-snap-project-info = Project details: { $detail }
feature-snap-param-table = Parameter table: { $detail }
feature-snap-mesh = Mesh { $id }: { $detail }
feature-step-accuracy = The accuracy must be between 0.000001 and 0.001 mm.
feature-step-total-size = The STEP shapes in the project are too large together ({ $size } MB, { $limit } MB allowed). Please import fewer or smaller bodies.
feature-step-import-reason = "{ $name }": { $detail }
feature-step-ids-used = The STEP shape ids are used up
feature-parts-prefix = Parts list: { $detail }
feature-parts-sheet-exists = There is already a parts list sheet.
feature-label-chamfer-equal = Chamfer with equal distance
feature-label-chamfer-two = Chamfer with two distances
feature-label-chamfer-angle = Chamfer with distance and angle
feature-label-box = Box
feature-label-cylinder = Cylinder
feature-label-sphere = Sphere
feature-label-cone = Cone
feature-label-extrude = Extrude
feature-label-revolve = Revolve
feature-label-step = STEP body
feature-mesh-locked = Mesh bodies cannot be edited
feature-err-unknown-feature = The feature does not exist in this document
feature-err-forward-reference = A feature may only refer to features that come before it
feature-err-face-not-found = Face "{ $role }" of feature { $origin } no longer exists
feature-err-edge-not-found = The edge between faces "{ $a }" and "{ $b }" no longer exists
feature-err-untagged-face = The face is not assigned to a feature and cannot be used
feature-err-untagged-edge = The edge is not assigned to a feature and cannot be used
feature-err-body-not-found = Body { $index } of feature { $feature } does not exist
feature-err-kernel = The kernel rejected the operation: { $detail }
feature-err-sketch = Error in the sketch: { $detail }
feature-err-implausible = The result is implausible (volume check failed)
feature-err-implausible-freeform = For bodies with freeform faces: the result is implausible (volume check failed); please change the position or size of a body slightly
feature-err-implausible-blend = The result of the fillet or chamfer is implausible (volume check failed)
feature-err-mesh-missing = Mesh { $id } is missing from the document
feature-err-step-shape-missing = STEP shape { $id } is missing from the document
feature-err-step-shape = The stored STEP shape cannot be read: { $detail }
feature-err-dependents = These features depend on it: { $ids }
feature-err-invalid-input = Invalid input: { $detail }
feature-err-out-of-range = { $what } is outside the representable range (absolute values at most { $max } mm)
feature-err-parameter-in-use = { $name } is used in { $users }.
feature-parts-name-taken = There is already a drawing named "{ $name }".
feature-parts-page-name-taken = There is already a drawing with a name like "{ $name }". Please rename it first so the continuation sheets of the parts list have room.
feature-label-sketch = Sketch
feature-label-boolean = Boolean
feature-label-move = Move
feature-label-mesh = Mesh body
feature-label-chamfer = Chamfer
feature-label-fillet = Fillet
feature-label-fillet-variable = Fillet with variable radius

## Error messages: drawings and dimensions
feature-max-drawings = A document may have at most { $max } drawings
feature-drawing-counter-used = The drawing counter is used up
feature-geo-edge-kind = This edge type cannot be dimensioned.
feature-geo-face-kind = This face cannot be dimensioned.
feature-geo-no-center = Only circles and arcs have a center.
feature-geo-no-range = The range of the cylindrical face cannot be determined.
feature-geo-mismatch = The reference does not match its type.
feature-geo-broken = The reference is damaged in the model.
feature-geo-face-gone = The face the dimension was attached to no longer exists.
feature-geo-edge-gone = The edge the dimension was attached to no longer exists.
feature-dim-ref-count = A dimension has the wrong number of references
feature-dim-ref-invalid = A reference of a dimension is invalid
feature-dim-role-long = The role of a face is too long (at most { $max } characters)
feature-dim-line-offset = The offset of the dimension line is outside the sheet
feature-dim-leader-angle = The angle of the leader line is invalid
feature-dim-text-shift = The shift of the dimension text is outside the sheet
feature-dim-decimals = A dimension may have at most { $max } decimal places
feature-dim-text-long = The custom dimension text is too long (at most { $max } characters)
feature-dim-fit-long = The fit designation is too long (at most { $max } characters)
feature-dim-tolerance = The tolerance of a dimension is invalid (absolute values at most { $max } mm)
feature-dim-counter-invalid = The dimension counter of the drawing is invalid
feature-dim-max = A drawing may have at most { $max } dimensions
feature-dim-id-invalid = A dimension has an invalid id
feature-dim-id-twice = A dimension id occurs twice
feature-dim-no-view = A dimension belongs to no view of the drawing
feature-dim-isometric = The isometric view has no dimensions
feature-dim-chain-mix = The dimensions of a chain mix chain and baseline dimensions
feature-dim-chain-same = The dimensions of a chain must be of the same type and in the same view
feature-dim-chain-order = The position in a dimension chain is too large (at most { $max })
feature-dim-chain-twice = A position in a dimension chain occurs twice
feature-dim-counter-used = The dimension counter of the drawing is used up
feature-dim-id-fixed = The id of a dimension cannot be changed
feature-drawing-unknown = The drawing does not exist
feature-dim-unknown = The dimension does not exist
feature-balloons-max = A drawing can have at most 200 balloons.
feature-drawing-name-needed = The drawing needs a name
feature-drawing-name-long = The name of the drawing is too long (at most { $max } characters)
feature-parts-page-number = A parts list sheet needs a page number from 1
feature-parts-sheet-empty = A parts list sheet has no views, no dimensions and no balloons
feature-drawing-views-max = A drawing may have at most { $max } views
feature-view-counter-invalid = The view counter of the drawing is invalid
feature-view-id-invalid = A view has an invalid id
feature-view-id-twice = A view id occurs twice
feature-view-scale = The scale must be between 1:1000 and 1000:1
feature-view-position = The position of a view is outside the sheet
feature-view-derived-aligned = A derived view must stay attached to its alignment
feature-view-no-parent-aligned = A view without a parent view cannot be aligned
feature-view-offset = The offset of a view is outside the sheet
feature-view-parent-first = The parent view must come before the derived view
feature-view-parent-unknown = The parent view does not exist
feature-view-isometric-derive = Nothing can be derived from an isometric view
feature-view-not-derivable = This view cannot be derived
feature-section-plane = The section plane does not fit the parent view.
feature-section-scale = A section view has the scale of its parent view.
feature-section-letter = A section letter is invalid or used twice.
feature-section-offset = The position of the section plane is invalid.
feature-balloon-counter = The balloon counter is invalid
feature-balloon-id-invalid = A balloon has an invalid id
feature-balloon-id-twice = A balloon id occurs twice
feature-balloon-no-view = A balloon belongs to a view that does not exist
feature-balloon-target = A balloon points to a place outside the view
feature-balloon-position = A balloon is outside the sheet
feature-drawing-name-taken = A drawing with this name already exists
feature-drawing-id-fixed = The id of a drawing cannot be changed
feature-drawing-kind-fixed = The type of a drawing cannot be changed
feature-view-unknown = The view does not exist
feature-balloon-place = The position of the balloon is invalid
feature-drawing-counter-invalid = The drawing counter is invalid
feature-drawing-id-invalid = A drawing has an invalid id
feature-drawing-id-twice = A drawing id occurs twice
feature-drawing-prefix = Drawing "{ $name }": { $detail }
feature-drawing-name-twice = A drawing name occurs twice
feature-parts-pages-gap = The parts list sheets must have pages 1, 2, 3 … without gaps
feature-parts-sheet-name = A parts list sheet has the wrong name

## Error messages: formulas and parameters
feature-expr-empty = The formula is empty.
feature-expr-too-long = The formula is too long (at most 200 characters).
feature-expr-too-deep = The formula is nested too deeply (at most 64 levels).
feature-expr-end = The formula ends unexpectedly.
feature-expr-no-close = A closing parenthesis is missing.
feature-expr-div-zero = Division by zero.
feature-expr-sqrt-neg = The square root of a negative number does not exist.
feature-expr-sqrt-unit = "sqrt" does not fit the unit here.
feature-expr-pow-unit = A length or an angle can only be raised to an integer power.
feature-expr-pow-exp-unit = The exponent must not have a unit.
feature-expr-mix = Length and angle cannot be added together.
feature-expr-units-differ = The units do not match.
feature-expr-value-too-long = The value cannot be written as a number with at most 200 characters.
feature-expr-not-finite = The result is too large or not a number.
feature-expr-name-empty = The name must not be empty.
feature-expr-name-form = A name starts with a letter or "_" and contains only letters, digits and "_".
feature-expr-name-long = The name is too long (at most 64 characters).
feature-expr-too-many = At most 500 parameters.
feature-unit-length = Length
feature-unit-number = Number
feature-unit-area = Area
feature-unit-volume = Volume
feature-unit-other = another unit
feature-unit-a-length = a length
# The two keys differ only in the German case (accusative / nominative).
feature-unit-an-angle-acc = an angle
feature-unit-an-angle = an angle
feature-unit-a-number = a number
feature-expr-unexpected = Unexpected character "{ $char }" at position { $pos }.
feature-expr-no-function = The function "{ $name }" does not exist.
feature-expr-needs-two = "{ $name }" needs at least two values (separate values with ";" or ", ").
feature-expr-needs-one = "{ $name }" needs exactly one value.
feature-expr-units-mismatch = The units do not match: { $na } and { $nb } cannot be added together.
feature-expr-no-name = The name "{ $name }" does not exist.
feature-expr-needs-angle = "{ $name }" needs an angle or a number.
feature-expr-expected = Here { $expected } is expected, the formula gives { $actual }.
feature-expr-reserved = "{ $name }" is already a function or constant.
feature-name-exists = The name "{ $name }" already exists.
feature-param-self = The parameter "{ $name }" depends on itself.
feature-param-cycle = The parameters { $list } depend on each other in a cycle.
feature-param-prefix = Parameter "{ $name }": { $detail }
feature-param-user = Parameter { $name }
feature-comment-too-long = The comment is too long (at most 200 characters).
feature-param-unknown = The parameter does not exist in this document
feature-param-ids-used = The parameter ids are used up
feature-value-length = { $field } gives { $value } mm, it must be greater than 0
feature-value-scale = Scale gives { $value }, it must be between 0.000001 and 1,000,000
feature-value-angle = Angle gives { $value }°, it must be between 1° and 179°
feature-value-dimension = A dimension gives { $value }{ $suffix }: { $reason }
feature-formula-in-field = { $name }, { $field }: { $msg }
feature-formula-in-dim = { $name }: { $msg }
feature-field-width = Width
feature-field-depth = Depth
feature-param-replace-blocked = Parameter "{ $name }" cannot be replaced by its value: "{ $feature }" currently has an error. Fix the error first.
feature-unit-angle = Angle

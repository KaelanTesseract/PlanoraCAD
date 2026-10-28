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
blend-mixed-bodies = The edges belong to different bodies ({ $names }); only one body can be done at a time
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
props-closed-at-import = Closed when read
props-yes = yes
props-no = no
props-at-import = When read: { $report }
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

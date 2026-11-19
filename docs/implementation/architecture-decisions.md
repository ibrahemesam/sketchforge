# Architecture decisions for draw.io parity

This records the decisions that shape the parity work. Each decision states the context,
the choice, the consequences, and the alternatives that were rejected. The decisions are
binding for milestones in `implementation-roadmap.md` until a later decision supersedes
one explicitly.

Baseline: commit `5de2f5b`. The uncommitted working tree is not part of this record.

## Existing architecture in one page

- **Document.** `Document` is an arena of `Node` values keyed by `NodeId`, with `root` and
  per-node `children` holding paint order, bottom to top. `world_rect` is derived by
  walking parents. Node styles, fills, text style, layout, component links, and visibility
  live on the node. `src/model/document.rs`, `src/model/node.rs`.
- **Editor.** `Editor` owns the active `Document`, `History`, camera, selection, gesture,
  tools, clipboard, and page slots. Inactive pages park their document, history, camera,
  and selection in `PageStash`. `src/editor/mod.rs`, `src/editor/pages.rs`.
- **History.** Edits run through transactions. A transaction owns a document snapshot
  boundary and records one undo step. Some UI state is intentionally outside history.
  `src/history.rs`.
- **Layout.** Auto layout and constraints derive positions from frame settings and child
  sizing. Groups derive bounds from children. `src/layout.rs`, `src/editor/structure.rs`.
- **Rendering.** `draw.rs` turns nodes into draw ops. `tess.rs` turns ops into meshes.
  `scene.rs` caches and culls by zoom bucket. Export reuses draw ops where it can.
  `src/render/`.
- **Connectors.** Bindings store a target node and a normalized point. Route selection,
  elbow A*, arrowheads, and binding gaps live in `src/connector.rs`.
- **Files.** A project is a folder with `project.json` and one page file per page. Page
  files carry `format` and `version`; loading runs pure migration steps. `src/project/`.
- **Format panel.** `src/ui/inspector.rs` builds Style, Text, and Arrange tabs plus a
  Canvas section. It reads the selection directly and writes through editor actions.
- **Scope boundary.** The editor is local-first. Network features are the macOS update
  check and the loopback MCP server. Diagram content is not sent anywhere.

## ADR-001: Keep the native document model, not mxGraph XML

**Status:** Accepted.

**Context.** draw.io stores diagrams as mxGraph cells with style strings and geometry in
XML. Sketch stores nodes in a typed arena with native fields for fills, text, layout,
bindings, components, and history. The two models overlap on rectangles and edges but not
on auto layout, components, rich text, or Sketch's transaction boundaries.

**Decision.** The native `Document` remains the only runtime and storage model. mxGraph
XML is an import and export format at the boundary, handled by an adapter. No code path
deserializes mxGraph cells into the live editor, and no feature stores mxGraph style
strings as the source of truth.

**Consequences.**

- New draw.io-compatible features need a native field and a documented mapping, not a
  pass-through style string.
- Some draw.io properties will have no native equivalent and must be reported as loss.
- The adapter can evolve without destabilizing the editor.
- Tests can use native fixtures for behavior and small XML fixtures for compatibility.
- Connection anchors reuse the normalized binding point: the eight corner and edge
  midpoint anchors snap within a screen-pixel tolerance, and the stored fraction keeps
  an attached point on that anchor across move and resize.

**Rejected alternatives.**

- Adopt mxGraph XML as the project format. It would discard typed layout, components,
  and migration guarantees, and make every editor change an XML mutation.
- Embed mxGraph or a JavaScript shape runtime. It conflicts with the native Rust app,
  adds a scripting and network surface, and would pull in draw.io code and assets.
- Store unknown mxGraph styles in a bag on every node and interpret them later. It hides
  loss instead of reporting it and creates two style systems.

## ADR-002: Page settings belong to the page document

**Status:** Accepted.

**Context.** At HEAD, grid size is an app-wide value in `options.json` and
`Editor::grid_step`. draw.io treats grid, page view, background, paper size, and global
style as diagram settings. A project can have pages with different paper and background.
Sharing one app-wide grid makes page parity impossible.

**Decision.** Add page-scoped settings to the page document, serialized in the page file.
The initial set is grid visibility, grid size, grid color, background color, background
image reference, paper size, orientation, diagram-wide shadow, and the diagram-wide
sketch default. Page view is a session-only view option, not document data, matching the
way draw.io exposes it in the View menu. App preferences remain in `options.json` and
only supply defaults for a new page. Settings changes go through history when they are
user edits.

**Rulers and manual guides (working tree, 2026-11-14).** Two more page settings follow
the same rule: a Rulers visibility flag and a manual guide list. A guide stores an axis
and a world-space position, so a page can hold vertical and horizontal guides at any
coordinates; the list survives a saved page and reload, and the Rulers flag decides
whether the strips draw without discarding guides already placed. Creating, moving, and
deleting a guide are editor transactions, so each is one undo step, and a drag that ends
on a ruler relabels its step as a delete. Manual guides are committed page data, separate
from the transient alignment guides drawn during a drag; for move and resize snapping
they take precedence over center alignment, equal spacing, and grid snapping, and a
resize snaps only the dragged edge. Rulers stay a view aid with no data, which is why
their tick labels are derived from zoom and camera rather than stored.

**Adaptive colors (working tree, 2026-11-18, corrected in round 34).** STY-22 adds
an adaptive mode to the same page settings: auto, simple, or none. Auto keeps
authored colors and explicit dark pairs, Simple flips only neutral black and
white, and None draws the page light even in the dark theme. Light values stay
authored in the page file; fills, strokes, gradient stops, text, labels, and page
background can carry optional dark values. Colors resolve when
`Document::with_appearance` supplies an appearance, so the renderer, scene cache,
and exporters see resolved colors without rewriting stored data. Text visibility
is handled by the renderer against the real text background. It lifts only
low-contrast glyphs on the dark canvas and leaves text-shadow glyphs alone. The
lift targets 14:1 contrast, so default ink reads as white on dark instead of a
middle gray. Exports keep authored colors. The Diagram tab edits the mode and
dark values through the usual history path. The app theme stays display state,
not a source of saved style changes.

**Shadows are defaults with explicit overrides (working tree, 2026-11-18).**
STY-13 and DOC-13 keep a page-wide Shadow flag and an optional per-element flag
as separate stored state. The element value wins when present, so a global
shadow never rewrites a node style. Draw ops offset a translucent copy of the
fill and stroke by (3, 4). Export bounds and scene culling include that offset,
while hit testing uses the unshifted bounds. draw.io import and export map the
flag as shadow=1 or shadow=0.

**Consequences.**

- Page files gain a version bump and a migration that inserts defaults.
- `Document` grows a settings field, or a settings module owned by `Document`.
- Undo and autosave include settings changes.
- Page view starts off, is not written to the page file, and does not carry across a
  reload. Turning it on draws the paper boundary for that session only.
- Export reads the active page's paper and background rather than a global option.
- Two pages can legitimately render with different grids and paper.

**Rejected alternatives.**

- Keep settings app-wide. It cannot represent draw.io pages and would make background a
  global lie.
- Store page settings in a sidecar file. It adds a second source of truth and a second
  atomic-write path.
- Put settings in `UiState`. They would not survive reload or travel with the project.

## ADR-003: Global restyling is an explicit, scoped, undoable command

**Status:** Accepted.

**Context.** draw.io's Diagram tab offers global Sketch, Rounded, Curved, Reset, and style
schemes that can change every element. A naive implementation would rewrite all node
styles on a checkbox change. That destroys per-element choices and can silently override
component instances.

**Decision.** A diagram-wide restyle is a command, not a side effect of a toggle. It has an
explicit scope (page or selection), a preview or count of affected elements, one undo
transaction, and a report of skipped elements. It does not clear rich-text spans, metadata,
component overrides, or per-corner radii unless the command explicitly targets them.
Theme switching changes chrome and display colors only, never stored shape colors.

**Consequences.**

- The global Style section in the Diagram tab needs a confirmation or preview step.
- Locked and hidden elements are skipped by default and counted.
- Component instances need an explicit choice: update the main, update overrides, or
  skip. Silent override loss is not allowed.
- A restyle can be undone as one step, including its report.

**Rejected alternatives.**

- Mutate styles in place whenever a global checkbox changes. It is destructive and hard
  to reason about with components.
- Rewrite only the top fill and stroke and leave everything else. It produces a partial
  restyle with no explanation.
- Make theme switching rewrite shape colors for dark mode. It breaks the stored light
  palette and makes exports theme-dependent.

**Dark colors are not a restyle (working tree, 2026-11-18).** STY-22 resolves optional
dark values at render and export time while the light values remain authored page
data. Switching the app theme changes appearance and the scene cache, not node styles.
A page can choose auto, simple, or none. None keeps every color light even when the
app theme is dark. This leaves the decision above intact: a theme change never
silently rewrites a diagram.

**Scoped schemes implement the restyle command (working tree, 2026-11-18).** STY-21
adds an explicit page or selection scope to scheme application. A SchemeReport
previews and then reports changed, locked, hidden, out-of-scope, and unstyleable
nodes. Locked, hidden, and group nodes are skipped by default. The command keeps rich
text spans, metadata, component instance overrides, per-corner radii, and adaptive
color pairs, and records the whole page-scope or selection-scope edit as one undo
step. Selection scope leaves page settings alone. draw.io export writes the scheme
name as sketchScheme on mxGraphModel and import validates it against the named
schemes. This realizes the ADR decision without adding a second destructive path.

## ADR-004: Persist explicit versions and pure migrations

**Status:** Accepted.

**Context.** Page files already carry `format` and `version`, and loading runs pure
`Value -> Value` steps. The current page version is 3. The parity work adds page settings,
layers, labels, and metadata over several milestones. Users keep old projects.

**Decision.** Keep the versioned migration chain. Additive fields may use serde defaults
when absence is unambiguous. Structural changes, including page settings, layers, and
connector labels, bump the page version and add a pure migration step with a fixture.
Loading refuses a version newer than the app supports. Migrations run on load; saving
writes the current version. No migration is performed lazily from the UI.

**Consequences.**

- Every schema change has a fixture from the previous version and a round-trip test.
- A single page file can move from v1 to the current version in one load.
- Fields that cannot be represented in an older version are written only in the new one.
- The app never opens a partially migrated document.

**Rejected alternatives.**

- Rely only on serde defaults. It works for optional fields but not for renamed or
  restructured data, and it hides version skew.
- Migrate files in place on first open without a version bump. It makes rollback and
  support harder and can lose data on a crash.
- Support forward-compatible unknown versions. A newer file may contain semantics the
  app would silently drop.

## ADR-005: Unsupported states are explicit

**Status:** Accepted.

**Context.** draw.io has hundreds of capabilities, and Sketch will not reach all of them.
The parity inventory contains 59 discovered and 34 blocked rows at baseline. A dead
control or a silent import drop is worse than a missing feature because it teaches users
to distrust the app.

**Decision.** Every unsupported or partially supported behavior is explicit in one of
three places: the UI, the import or export result, or the documentation. Controls that
cannot work are not shipped as clickable no-ops. If a control is shown disabled, it
carries a reason. Import and export adapters return counts of imported, approximated,
and dropped content. The inventory uses the status vocabulary in ADR-014.

**Consequences.**

- Some draw.io controls will be absent from Sketch until they work. That is acceptable.
- Import is allowed to refuse a file rather than produce a misleading partial document.
- The panel can show a short "Not supported yet" note with a link to the inventory ID.
- Support questions have a written answer instead of a guess.

**Rejected alternatives.**

- Show every draw.io control and make unsupported ones no-ops. It looks like parity and
  breaks trust.
- Hide all gaps from the UI and document them only in a changelog. Users cannot tell
  whether a missing result is a bug or a known limit.
- Implement a shallow version of every feature. It multiplies maintenance and produces
  wrong output in exports.

## ADR-006: No proprietary assets and no copied draw.io implementation

**Status:** Accepted.

**Context.** Sketch is a clean-room reimplementation. The repository bundles Inter and
Noto Sans Arabic under the SIL Open Font License and ports MIT-licensed algorithms with
attribution. draw.io is open source, but its documentation, shape libraries, templates,
and icon packs include assets with separate terms. Vendor stencils carry trademarks.

**Decision.** Use draw.io documentation for behavior only. Do not copy draw.io code,
shaders, icons, fonts, templates, or shape XML into the application. Native shapes are
built from Sketch's own geometry. Third-party code or assets require a named source, a
license review, an entry in `THIRD_PARTY.md`, and a comment where the code starts. Vendor
icon packs (AWS, Azure, SAP, Citrix, Salesforce, Veeam, and similar) stay blocked until a
license decision exists. Documentation links may point to draw.io pages.

**Consequences.**

- "Feature parity" means behavior parity, not visual asset parity.
- Domain packs use generic native shapes unless a licensed asset set is approved.
- Imported draw.io shape XML is treated as data to map, not as code to execute.
- A clean-room claim remains defensible.

**Rejected alternatives.**

- Copy draw.io's shape definitions or stencil XML. It conflicts with the clean-room rule
  and may carry asset licenses.
- Bundle vendor icons because draw.io does. Trademark and redistribution terms differ by
  vendor and are not covered by the app license.
- Reuse draw.io screenshots or icons in the UI. The UI should use Sketch's own icon set.

## ADR-007: The format panel is a context-driven projection

**Status:** Accepted.

**Context.** The inspector already has Style, Text, and Arrange tabs, but it reads the
selection ad hoc and always shows some rows. draw.io changes the panel by selection type
and shows diagram settings when nothing is selected. Adding labels, layers, shape
properties, and import options to the current structure would create a second, implicit
context model.

**Decision.** Introduce a pure selection context resolver. It maps the current selection
to a context: document, shape, text, connector, frame, group, component, instance, or
mixed. A section registry declares which sections apply to each context. The panel renders
those sections in a stable order. Section open state lives in `UiState`. Page settings live
in the document, not the panel state. Every control calls the same editor action as the
command palette and shortcuts.

**Consequences.**

- The panel can be tested as a matrix without a GPU.
- A connector and a text layer can have different tabs without duplicating code.
- Adding a feature means adding a section and a context rule, not another `if selected`.
- The panel file can be split before it becomes a monolith.

**Rejected alternatives.**

- One long scroll of every property. It is hard to scan and hides irrelevant controls.
- Separate dialogs per feature. It fragments editing and duplicates selection state.
- Encode panel state in the document. UI preferences would travel between machines and
  pollute the file format.

## ADR-008: Connector labels are owned by the connector

**Status:** Accepted.

**Context.** Sketch has no connector labels. draw.io labels are child cells with geometry,
which allows several labels and free positioning. Sketch's selection, copy, and export
assume nodes are the unit of content. Making a label a child node would add a second
selection and hit-test path for every edge.

**Decision.** Connector labels reuse the label fields every node already has: `label`,
`label_color`, `label_size`, `label_wrap`, `text_bg`, `text_border`, `label_opacity`,
and `text_style`. A connector is now a label target like a shape. `text::layout_node`
lays a wrapped label out in an 80 point column, and `text::text_origin` centers that
column on a route point. `connector::line_path` supplies the same geometry to the
canvas, SVG, and PDF, so the label sits at the same point everywhere.

Two optional fields on the node place the label. `label_pos` is the normalized distance
along the flattened route in `0..1` (`None` means `0.5`). `label_offset` is a world-space
displacement from that route point (`None` means on the line). `connector::path_point_at`
walks the flattened path; `connector::closest_point` returns the nearest point and its
normalized distance. `text::label_box` measures the label in node space and is shared by
hit testing, bounds, and drag handling.

Double-click opens the label editor before the connector's bend handle can take the
click. A labelled connector hides its midpoint bend handle because the label owns that
spot; the endpoint handles still move and rebind it. Pressing a label starts
`Gesture::LabelDrag`; once the pointer passes the drag threshold, the editor opens one
`Move label` transaction, projects the pointer onto the route, and stores the remainder
as offset. The label can therefore slide along the line or sit off it and still follow a
reroute. Hit testing accepts the routed line and the measured label box. The Text tab
treats connector labels as label targets and hides the box-only position and spacing
rows. Settings live in the existing label fields, so save, load, copy, undo, clear
formatting, and export need no new serialization path.

Arrange > Reset has `Label Positions`, which clears `label_pos` and `label_offset`, and
`Waypoints`, which drops manual bends from straight and curved connectors. Each command
opens one transaction and does nothing when the selection has nothing to reset.

**Deferred.** Line jumps belong to later routing work.

**Reverse connector (2026-10-26).** Arrange tab gains a Reverse button that swaps each
selected connector's source and target. It follows draw.io's `turnShapes` edge branch:
swap the terminal bindings, reverse the waypoint list, and leave arrowheads and label
positions alone. Because heads and `label_pos` are bound to the line's own ends, both
visually move to the opposite end while the path stays where it is; draw.io documents
this as "your connector ends and their labels will be swapped, but the path of the
connector will not be changed". An earlier draft also swapped `start_head`/`end_head`
and mirrored `label_pos`, which preserved the picture exactly and made the command a
no-op on canvas; that draft was discarded after checking the draw.io source. The command
is one `Reverse connector` transaction, so undo and redo restore the previous ends in a
single step.

**Transparent line gap (2026-10-26).** A label without a solid background now cuts the
line behind its text. The gap is derived state, not a stored field: `text::label_gap_rect`
returns the padded glyph box when the connector has label text and `text_bg` is `None`,
and `connector::cut_line_path` removes the spans inside that rectangle. A solid
`text_bg` plate keeps painting over one continuous line, because the plate already hides
it. `cut_line_path` flattens the route, simplifies it with Ramer-Douglas-Peucker at the
same 0.25 tolerance the flattener uses, clips each span against the rectangle
(Liang-Barsky), and rebuilds the path as runs joined by moves. When nothing is cut the
original path is returned unchanged, so smooth routes keep their exact curves. The crisp
renderer and the sketch renderer share the cut, and SVG, PDF, and PNG export inherit it
because they render the same draw ops. The gap follows label moves and reroutes for free
because it is computed from the same layout the label uses.

**Multiple labels per connector (2026-10-27).** The planned extension landed as a
label list that keeps the single-label fields intact. The first label stays in
`label`, `label_pos`, and `label_offset`, so pre-existing documents and every existing
consumer keep working unchanged. Second and later labels live in a new `labels` vector
on the node; each entry carries only its text, its own `pos`, and its own `offset`,
and inherits the connector's label style (color, size, wrap, background, border,
opacity, font), matching draw.io where an edge's extra labels copy the edge style. The
vector is skipped in serialization while empty, so nothing changes for connectors with
one label. Extras never sit empty: clearing an extra's text removes it when the edit
ends.

Consumers iterate a unified label view by index: index 0 reads the legacy fields, and
higher indices read `labels[idx - 1]`. The text, origin, box, and gap helpers gained
per-index forms (`layout_node_label`, `text_origin_label`, `label_box_label`,
`label_gap_rects`), the renderer draws one text op per label, `draw_bounds` unions
every label box, and `cut_line_path` and `jumped_line_path` take a slice of hole
rectangles so several labels cut several stretches of the line; overlapping holes
merge into one stretch. `TextEdit` and `Gesture::LabelDrag` carry the label index, and
`connector_label_at` returns the index of the topmost label under the pointer, so
double-clicking a label on the canvas opens that label's editor. The Text tab gains an
`Add label` button for a single selected connector; the new label starts centered on
the line with placeholder text selected, so typing replaces it, and the add is one
`Add label` transaction followed by the usual `Edit text` step for the typing.
`reset_connector_label_positions` resets extras as well. The MCP surface reports
`labelCount` and a `labels` array with per-label position and offset, and
`set_text_content` accepts a `labelIndex`.

**Source connector defaults (2026-11-17).** A non-group node can store a source connector
default: the style and text style for new connectors started from that shape. The Style tab
copies the selected connector's own style to the shape its line starts from, while the
Arrange tab copies the page connector default to selected shapes or clears their source
default. All three commands are explicit transactions that join undo and redo. Connector
creation resolves the default from the start binding first and the page second, so a shape
override wins without rewriting the page default or changing connectors already on the
canvas. The same resolver serves ordinary arrow drags and quick-connect arrows. Existing
connectors keep their own style; the default only changes what a new line inherits.

The value is native document data, skipped while absent, and travels with copy, page save,
and reload. draw.io export writes it as a JSON string in the UserObject sidecar and import
reads it back, keeping the sidecar key out of the visible metadata map. draw.io itself has
no equivalent command for a shape-owned connector style, so the exported cell still shows
the style on newly drawn edges only when reimported into Sketch.

**Flow animation (2026-11-18).** Flow settings live on the node, not inside the connector
kind, so one constructor supplies defaults for every shape and old documents deserialize
without a migration. Only connectors read the setting. The renderer turns it into a dash
offset from the editor clock, so animation needs no timer in the document and exports stay
still. Animated connectors skip the scene cache because their meshes change every frame.
The editor asks for repaints while any visible flowing connector exists. draw.io maps
flowAnimation, flowAnimationDuration, flowAnimationTimingFunction, and
flowAnimationDirection, with flowDuration accepted on import. The native page file skips
the default flow value, so older files and new files without animation stay byte-shape
compatible for this field.

**Consequences.**

- Copy, delete, undo, and export move with the connector, not with a hidden child.
- The route needs a stable length parameterization for straight, curved, and elbow paths.
  `connector::route_polyline` flattens the route once, and `path_point_at` and
  `closest_point` share it.
- A label can be dragged off the line without changing the route because position and
  offset are separate fields.
- Reset commands are ordinary document edits, so they participate in undo and save.
- Multiple labels are a list, not a new node type.
- Text style reuse keeps label typography consistent with text layers.
- The current single string is the list of one until the extension lands.

**Rejected alternatives.**

- A label child node with its own parent link. It complicates z-order, selection, copy,
  and export and can become detached from the edge.
- Use HTML labels. There is no HTML layout or sanitizer, and it would make export and
  hit testing depend on a browser engine.
- Separate connector-label fields beside the shape-label fields. It duplicates layout,
  styling, serialization, and inspector code for no benefit.

## ADR-009: Layers are page-scoped membership, not z-order

**Status:** Accepted.

**Context.** Sketch's Layers panel is a z-order tree built from parent and child arrays.
draw.io layers are a separate page-level structure with visibility, lock, and ordering.
Connectors may cross layers. Renaming the existing tree "layers" would make both concepts
ambiguous.

**Decision.** Add page-scoped layer records with id, name, visibility, lock, and order.
Every node has an optional layer id. A missing id inherits the nearest ancestor's layer,
and a root without an id uses the default Background layer. A subtree must have one
effective layer. Moving a selection to another layer writes that layer id across the whole
subtree. Paint order is layer order first, then root order inside the layer, with children
following their parent. Hiding or locking a layer composes with the existing per-node
visible and locked flags. Connectors may bind to nodes on other layers. The existing
Layers panel stays the z-order tree; a separate Layers dialog manages layers. Both are
labelled to avoid confusion.

**Layers in the working tree (2026-10-27).** This ADR landed uncommitted on
`agent/drawio-features`. `Layer` holds the id, name, visible, and locked record, and a
page keeps a bottom-first layer list plus a next id, so the v4 to v5 page migration only
inserts the Background layer and old versions load through the existing chain. A node
stores its layer id and children inherit the nearest ancestor's, which document
validation enforces, so one subtree sits on one layer. `layered_root` buckets the
unchanged root list by layer, and the canvas, hit testing, and the SVG, PNG, and PDF
exports walk that order. Add, rename, duplicate, delete with its shapes, reorder,
visibility, lock, select all on a layer, and moving a selection to a layer are each one
undo step whose snapshot also carries the layer list and the next id, so undo restores
them too. Connectors keep their bindings across layers. The Layers dialog manages the
page as decided here, and the z-order Layers panel keeps its role.

**Consequences.**

- A child cannot sit on a different layer from its parent. This keeps clipping and world
  positions correct. It is a documented difference from a per-cell layer model.
- Paint order, hit testing, and export must agree on the layer-first order.
- A page that has never used layers gets one default layer in migration.
- The existing visibility and lock code composes instead of being replaced.

**Rejected alternatives.**

- Treat the current root order as layers. It cannot hide a whole layer independently and
  conflates grouping with page organization.
- Allow every node an independent layer. A child painted outside its parent's clip would
  break frames and groups, and hit testing would disagree with rendering.
- Make layers global across pages. draw.io layers are per diagram page.

## ADR-010: Shape variety comes from a native registry

**Status:** Accepted.

**Context.** The closed `NodeKind` enum covers rectangles, ellipses, polygons, stars,
frames, text, connectors, stickies, paths, and strokes. draw.io's breadth comes from
libraries of shape definitions. Importing arbitrary mxGraph shape XML would turn the
editor into an interpreter for a foreign format.

**Decision.** Add a native shape registry. A registry entry has a stable id, name,
category, keywords, preview builder, geometry builder, default style, and allowed
connection points. Entries emit existing native kinds or editable vector paths. Search
returns entries by fuzzy name and category. Library items may reference a registry entry.
Custom shape XML from draw.io is mapped by an adapter when it can be expressed as native
geometry; otherwise the import report lists it as unsupported. Registry entries cannot
execute code, fetch a URL, or carry an unlicensed asset.

**Landed (working tree, 2026-11-05).** The registry is a static table of 30 entries in
six categories. Each entry has a stable id, name, category, keywords, preferred size, and
a plain-style flag, and its builder emits an existing `NodeKind` or a `NodeKind::Path`
assembled from the shared vector primitives. Nothing is interpreted at runtime and no node
field was added: an insert is an ordinary node with a normal style, frame membership, and
layer. Entries marked plain take the page's remembered shape default on insert and in the
search preview, while stroke-art entries keep their own pen style. Search is a local
subsequence matcher with word-start and run bonuses, so it needs no network and no index.
The Library tab, quick add, and MCP `insert_shape` all go through one editor entry point,
which keeps centering, parenting, layer assignment, selection, and a single undo step in
one place. Autosize complements the registry by making containers hug whatever a search or
a drag inserted.

**Templates (working tree, 2026-11-11).** Complete diagrams are data on top of the
registry, not a second renderer. A template library is a versioned JSON file holding
named templates with a category, description, keywords, an optional SVG thumbnail, and
one or more pages. A page stores ordinary nodes, root order, diagram settings, and
layers, so insertion produces nodes the editor already knows how to select, restyle,
connect, export, and undo. A node may record the registry shape it was built from; when
that shape is absent from the running build, insertion names it in the status line and
leaves only that node out, while the rest arrives and its bindings, child references,
and instance links are cleared so no dangling id survives. Bundled packs are pure
builders over native kinds and registry entries: fifteen of them cover flowchart, UML,
ER, BPMN, network, floor plan, org chart, kanban, timeline, Gantt, C4, SysML, charts,
Venn, and a multi-page project plan. No template carries an image, an icon, a network
request, or executable content, so the clean-room rule holds. A custom folder chosen in
the browser is an app option; every JSON library in it loads beside the bundled packs,
and the open page can be saved back as a template with a generated thumbnail. Insertion
goes through the existing library and page paths, which keeps centering, layer mapping,
selection, and a single undo step in the editor rather than the dialog.

**Search omnibox (working tree, 2026-11-12).** The Ctrl+K palette is the registry's
second search surface. It ranks every registry entry beside commands, tools, pages,
layers, templates, generators, and library items, and labels a shape entry with its
category. Choosing one calls the same editor insert path as the Library tab and quick
add, so the result is an ordinary node at the viewport center with selection and one
undo step. Registry search stays local and needs no index. The same field also lists
diagram text from every page through the find engine, covered by ADR-013, so users do
not have to know whether a name belongs to a command, a shape, or a label.

**Consequences.**

- Domain packs become data and small geometry builders, not a second renderer.
- A shape that needs new behavior, such as a table, gets an explicit NodeKind or model,
  not a generic scripting hook.
- Search and previews share one catalog.
- Import coverage is measurable against the registry.

**Rejected alternatives.**

- Embed mxGraph's shape vocabulary. It adds a large foreign runtime and blocks clean-room
  review.
- Make every shape a vector path. It loses semantic editing such as table rows, ports,
  and connector anchors.
- Hard-code each stencil as a toolbar button. It does not scale to search or libraries.

## ADR-011: Import and export use adapters with loss reports

**Status:** Accepted.

**Context.** Sketch can import Excalidraw and SVG and export PNG, SVG, and PDF. draw.io
XML, VSDX, Gliffy, and Lucidchart are absent. Each format has concepts the native model
does not share. A direct deserializer would either reject useful files or drop data
silently.

**Decision.** Every external format gets an adapter with three stages: parse into an
intermediate representation, map to native content, and produce a report. The report
counts imported, approximated, and dropped items with reasons. The adapter never writes
outside the new project folder and never executes content. New XML formats use a real XML
parser, not regular expressions. Compressed draw.io payloads are decoded explicitly;
encrypted payloads are refused with a clear message. Export maps only the supported
subset and records what it skipped. Round-trip fixtures live with the adapter tests.

**Landed (working tree, 2026-11-08).** The draw.io adapter follows the three stages. It
parses `mxfile` and `mxGraphModel` XML with roxmltree into an intermediate cell list,
decodes compressed diagram payloads, and refuses encrypted files with a message that
names the diagram. Mapping preserves page names and order, turns layer cells into native
page layers with cell 1 as Background, converts parent-relative geometry into world and
child-local rectangles, normalizes groups to the union of their children, maps swimlane
and container cells to frames, builds connectors with bindings, waypoints, arrowheads,
dash, and extra labels, and resolves common draw.io shapes through the native registry.
Export writes every page into one uncompressed `mxfile` and maps only the supported
subset; native pen paths and freehand strokes leave as basic shapes or edges, which the
roadmap records. The report is part of the import result and the app shows it in a
dismissible window; the CLI prints the summary and each diagnostic. The fixture
`tests/fixtures/drawio/two-pages.drawio` and its tests keep the contract measured: two
pages with a layer, a group, a swimlane, mixed styles, and an elbow edge with an extra
label, geometry round-tripping within 0.01 world units. Encrypted payloads fail before
any page is written, and the adapter only writes through the caller's new project folder.

**Landed (working tree, 2026-11-11).** The local text-to-diagram generators follow the
same three stages without a file format on disk. Each parser reads text into a native
document, reports what it approximated or dropped, and never executes the input or
makes a network request. Mermaid flowcharts and sequence diagrams map to shapes,
connectors, participant boxes, lifelines, and labelled messages, with unsupported
directives reported rather than fatal. CSV rows map to shapes through a shared layout
helper with grid, flow, and tree placement, quoted fields, delimiter detection, and an
optional header and label column. SQL maps CREATE TABLE and ALTER TABLE statements to
frames with column text, key markers, and bound foreign-key connectors. JSON maps a
node and edge object into shapes and connectors, keeping explicit coordinates only when
every node has them and otherwise using a deterministic layered or grid layout. The
report is part of the generated result: insertion carries its summary into the status
line, and the MCP tool returns the warnings. A parser failure returns one message that
names the line or token, and no partial content is inserted. Generated content is an
ordinary document, so the editor inserts it through the same subtree path as library
and template content, on the open page in one undo step or as a new project page. Tests
cover a fixture, a failure message, and layout determinism for each source.

**Landed (working tree, 2026-11-18).** Embedded export data uses the same versioned
JSON structure as a native project bundle rather than a second document format. The
bundle holds the project name, active page id, every page with its camera, and the
bytes of image assets those pages reference. SVG carries base64 text in a metadata
element with a stable id; PNG carries base64 text in one tEXt chunk with a short
Latin-1 keyword. Export adds the carrier only when the user opts in. Existing export
calls default to no bundle, so files exported without the option keep their current
carrier and remain readable by existing import paths. Import checks the carrier
first and rebuilds native pages from the bundle before any vector importer runs. A
missing carrier falls through to the existing importer; a corrupt bundle returns a
clear error. The carrier is transport, not a compatibility claim: it does not write
draw.io's metadata format, and PDF and HTML export carry no source data.

**Adaptive colors (working tree, 2026-11-18).** The draw.io adapter reads
`adaptiveColors` into page settings and `light-dark(light,dark)` into light and dark
color fields for page background, fill, stroke, gradient stops, font, label
background, and label border. Export writes the mode and pairs back. PNG and SVG
export bake the appearance the caller selects, and the CLI accepts
`--appearance light|dark`. Resolution happens in the export path, so a saved project
keeps both values. Import and export tests check that explicit pairs survive a
round trip and that auto, simple, and none keep their values.

**Consequences.**

- Import can succeed with warnings, but never with silent loss.
- The UI has a place to show the report after import.
- Format support is measured by fixtures, not by a claim.
- The native model does not grow fields just to preserve an unsupported style string.

**Rejected alternatives.**

- Deserialize directly into `Document`. It couples native evolution to the foreign format
  and makes loss invisible.
- Best-effort import with console warnings. Users do not read the console.
- Preserve the original file and render it as an opaque blob. It defeats editing.

## ADR-012: The core editor makes no network requests

**Status:** Accepted.

**Context.** Sketch is local-first. The macOS build checks for updates through Sparkle, and
the MCP server listens on loopback. Diagram content, assets, and libraries stay on disk.
draw.io integrates with cloud drives, Git hosts, and collaboration services.

**Decision.** The core editor, document model, importers, exporters, and renderer make no
network requests. Remote images are not fetched. Integrations live outside the core,
behind an explicit opt-in and a separate process or service. The update check remains a
platform-specific feature that sends no diagram data. MCP remains bound to loopback and
rejects browser origins and foreign hosts. Any future feature that needs the network
requires a new ADR, a privacy review, and a way to disable it.

**Consequences.**

- Cloud storage, collaboration, share links, AI generation, and public library galleries
  are blocked until a product decision and a service exist.
- Import must work from local files and local data URLs.
- The app remains usable offline and testable without mocks for most of the editor.

**Rejected alternatives.**

- Add cloud sync directly to the document store. It introduces auth, conflict resolution,
  and a server into the core.
- Fetch remote images lazily. It leaks document contents and makes exports
  non-deterministic.
- Phone home for usage analytics. It contradicts the local-first promise.

## ADR-013: Metadata is a stable string map, with typed link and tag fields

**Status:** Accepted.

**Context.** draw.io shapes can carry arbitrary properties, links, tooltips, and tags.
Placeholders resolve from these values. Sketch has no generic data field. A free-form JSON
bag would be easy to add but hard to edit, validate, and export consistently.

**Decision.** Add an optional ordered string map to nodes and pages for user properties.
Use `BTreeMap` ordering so serialization is deterministic. Add typed optional fields for a
custom link, a tooltip, and a tag list rather than hiding them inside the map. Metadata
does not affect rendering by itself. Placeholder expansion is a separate feature with
cycle detection, a missing-value fallback, and no code execution. Import and export map
known keys and report the rest.

**Landed (working tree, 2026-11-10).** Nodes carry `metadata`, `link`, `tooltip`, and
`tags` with serde defaults and skip-empty serialization, so old page files load unchanged
and empty fields keep the file small. The inspector Data tab edits the link, tooltip, and
comma-separated tags; the Edit Data dialog edits shape or page properties as ordered rows
with add, rename, and remove. A transient page context carries the page name, number,
count, and file name, and the editor refreshes it on load, page creation, rename, delete,
and switch. The resolver in `src/model/placeholders.rs` checks the node map, then the
nearest ancestor map, then the page map; it also answers the draw.io predefined keys for
id, size, length, date and time masks, page name, page number, page count, and file name.
Unresolved names render as `«name?»`, and `%%` prints one percent. Values are inserted
literally and are never expanded again, so recursive placeholders and cycles cannot form.
Resolution happens at paint and export time only, so the page file keeps the template. Links are classified in
`src/link.rs`: internal targets are `data:page/id,<id>`, `data:page/<name>`, and
`page:<name>`; external targets must use http, https, or mailto. Script, file, data, and
scheme-less targets are refused. External links open through the platform opener and ask
for confirmation first. Find and replace live in `src/find.rs` and the find bar; one
replacement batch is one undo step per page, and locked or hidden nodes are skipped and
counted. The draw.io adapter reads UserObject attributes and a `sketch-data` sidecar and
writes them back, so metadata, links, tooltips, and tags survive export and reimport.

**Find results in the omnibox (working tree, 2026-11-12).** The find module's query,
field list, and match preview feed the Ctrl+K search field, so commands, shapes, and
diagram text appear in one ranked list. The omnibox does not add a second search
taxonomy: it calls `find::search` with the same case-insensitive literal rules, dedupes
matches per node field, and caps text hits at 40. Selecting a hit switches to the owning
page, selects the node, and zooms to it through the existing selection and camera
commands. Empty queries add no text entries, which keeps the palette's opening list short.

**Consequences.**

- Edit Data and the inspector can share one simple model.
- Deterministic ordering makes round-trip diffs stable.
- Links need a confirmation step and a URL scheme allowlist before they open.
- Placeholders cannot introduce scripts or remote content.

**Rejected alternatives.**

- A free-form JSON value per node. It complicates the editor and makes every consumer
  interpret types.
- Store metadata in the label or tooltip. It makes data visible and hard to query.
- Expand placeholders at save time. It destroys the template and makes edits lose data.

## ADR-014: Status claims follow a fixed evidence contract

**Status:** Accepted.

**Context.** A parity table with checkmarks invites overclaiming. Earlier work in this
repository has runtime evidence in `.opencode/MEMORY.md` that was not re-run in every
session. Some features are implemented in an uncommitted working tree. The inventory must
distinguish these cases.

**Decision.** Use exactly six statuses: `VERIFIED`, `IMPLEMENTED`, `IN_PROGRESS`,
`ANALYZED`, `DISCOVERED`, `BLOCKED`. `VERIFIED` requires a named committed test or a
directly exercised code path. `IMPLEMENTED` means code exists but no focused test or
this-session runtime check covers it. `IN_PROGRESS` means partial code at HEAD.
`ANALYZED` means behavior and design are documented with no code. `DISCOVERED` means the
behavior is documented and absent. `BLOCKED` adds a concrete blocker. Pin every inventory
claim to a commit. Record commands, dates, results, and unverified claims in
`verification-log.md`. Never count uncommitted files as implemented.

**Consequences.**

- The inventory is less flattering and more useful.
- A status change needs evidence, not a code read alone.
- Runtime notes can support a row but cannot replace a test for `VERIFIED`.
- Reviewers can reproduce the evidence path.

**Working tree note (round 35).** The STY-14 row moved to VERIFIED after five
focused tests and a recorded live run. The base commit stays pinned, and the
notes point to `.artifacts/round35/`. The live run used pointer input and MCP
because cua-driver was unavailable, and its report records that limit.

**Rejected alternatives.**

- A simple done or not-done column. It hides partial and blocked work.
- Count uncommitted work as done. It makes the baseline move under the reader.
- Treat a screenshot as verification for every platform. It proves one render, not the
  behavior under reload, undo, and export.

## ADR-015: Line jumps are a connector field resolved at paint time

**Status:** Accepted.

**Context.** draw.io lets an edge hop over the edges painted below it
(`jumpStyle` of `none`, `arc`, `gap`, `sharp`, or `line`, plus a `jumpSize`
input of 0 to 999, default 6). Whether a connector crosses another depends on
paint order and the other connectors' routes, so the hop cannot be stored as
geometry. The hop must also survive undo, serialization, and export.

**Decision.** Store `jump: LineJump` and `jump_size: f64` on the connector
kind with serde defaults, so old files load as `none` at size 6. Compute the
crossings at paint time: a shared `JumpWalk` walks the connectors in paint
order, remembers the routes drawn so far, and hands each connector the points
where it crosses one of them. Crossings within 0.5 world units of a span end
are ignored, so lines that meet or share a corner do not hop, and crossings
that land on the same spot collapse into one hop. The hop shapes follow
`mxConnector.paintLine`: the arc is a cubic whose controls sit 1.3 times the
half gap off the line, sharp steps square, line draws a tick on each side, and
gap interrupts the run. The half gap is `(jumpSize - 2) / 2 + strokeWidth`,
matching draw.io. The scene cache hashes the crossing points beside the node,
so a style change re-renders without invalidating everything. The SVG and PDF
exporters run the same walk over the exported roots.

**Consequences.**

- A connector only hops over connectors painted below it, as in draw.io.
- Per-node exports show no hops unless the crossed connector is exported too,
  because the walk only sees the exported roots.
- Jump rendering and label gap cutting compose: the label hole is cut out of
  the runs between hops.
- The rough renderer sketches the jumped path, so sketch mode keeps hops.

**Rejected alternatives.**

- Store hop geometry on the connector. It goes stale when the other connector
  moves and duplicates what paint order already knows.
- Hop in both directions. draw.io paints the jump on the edge painted last;
  bidirectional hops would double-draw every crossing.
- Compute crossings in the tessellator. It would run per frame and could not
  share the result with exporters.

## Open questions

1. Should page settings changes be undoable individually or grouped with the next edit?
   ADR-002 and T9 choose individual history entries. This needs a runtime check because
   rapid grid changes could flood history.
2. Should layer membership be a field on every node or only on root subtrees? ADR-009
   chooses effective subtree membership. The migration must enforce it and the UI must
   explain why a child cannot move alone.
3. Should connector labels reuse `TextStyle` spans, or use a simpler label style? ADR-008
   starts with the shared text style and no spans, then adds spans only if range editing
   on a label is implemented.
4. Should the draw.io adapter preserve unknown style strings in the loss report or in a
   sidecar for round-trip? ADR-011 prefers the report. A sidecar would help round-trip
   fidelity but adds a second model.
5. How much of the 220-page documentation corpus should become automated link checks?
   The current check validates the 231 rows against the crawl. It does not re-fetch the
   pages.

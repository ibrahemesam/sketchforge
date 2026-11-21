# Sketch agent session state

Last updated: 2026-11-21 (round 69 unlabeled connector tab differentiation, local pending commit/push).

## Objective

Evolve sketch-rs into a draw.io-parity diagram editor. Current spec: the draw.io
format sidebar (`drawio_features_opencode_spec.md`, in
`/home/ibrahem/mnt/Files/Downloads/`), then the milestone roadmap in
`docs/implementation/implementation-roadmap.md`.

## Current state

Branch `agent/drawio-features` tracks `fork/main`, which points at
`ibrahemesam/sketchforge` on GitHub. Round 36 implementation was pushed as
7261dfd with the repo presentation and MIT license. Round 37 implementation was
pushed as 858d1a0 with the first-run onboarding tour. Round 38 was pushed as
5802ded plus its docs closeout with paragraph lists and indentation. Round 39
was pushed as 97dfc51 plus its docs closeout with the print dialog and
paper-sized PDF output. Round 40 was pushed as a63e090 with the per-page
background image. Round 41 was pushed as fc0f065 with curved text and ff63722
with its docs closeout. Round 42 was pushed as 727c534 with a preserved custom
grid color, e9bf2ed with its docs closeout and c43a525 marking it pushed. Round
43 was pushed as 9cae03e and cb8a4fd with the page fade transition and b2f487f
with its docs closeout. Round 44 was pushed as 60186ab with draw.io shape
library XML import and export, 3fe9208 with its docs closeout and 4510e02
marking it pushed. Round 45 was pushed as c7941e0 with installed font families
exposed to MCP and draw.io, e7cce0a with its docs closeout and e6dcb21 marking it
pushed. Round 46 was pushed as 221cf0f with the MCP reverse_connectors tool,
904a29e with its docs closeout and 083ab91 marking it pushed. Round 47 was pushed
as 2ec2fbd with MCP tag search, a610877 with its docs closeout and 45df3b6
marking it pushed. Round 48 was pushed as cca0145 with tooltip truncation and
ac17535 with its docs closeout. The welcome text fix was pushed as a85f308 and
the product-focused README as 87fced5. Round 49 implementation was committed as ea87c41 with the `set_link` MCP tool
and its docs closeout was pushed. Round 50 implementation was committed as
30636ea with the draw.io `__sketchLabelUpright` sidecar round trip. Do not
create releases or deploy Vercel; plain `git push` sends commits to the fork.

The full sidebar spec was implemented and verified in round 6 (see
`.opencode/MEMORY.md`). Round 7 closed the transparent connector label gap.
Round 8 closed reverse connector. Round 9 fixed the user-reported design panel
resize bug. Round 10 closed line jumps (ADR-015). Round 11 fixed the three
user-reported FIXMEs (font persistence, magnetism scope, marquee containment).
Round 12 closed CON-11, multiple labels per connector (see ADR-008 extension).
Round 13 closed CON-21, auto-route around all obstacles: the elbow router now
treats every bindable, visible shape near the route as an obstacle, not only
the two bound shapes. Obstacles are the shapes' world boxes with a fixed
clearance (OBSTACLE_PADDING 10), culled to the route's search neighborhood so
the A* grid stays bounded, and their edges become grid lines the search paths
around. A shape moved into a connector's route neighborhood reroutes that
connector in the same transaction, so the avoidance is one undo step with the
move. Straight and curved routes are unchanged, matching draw.io where only
orthogonal routing avoids shapes. Round 14 closed M3 page layers (see ADR-009): every
page starts with a Background layer, old pages get one through the v4 to v5 format
migration, a node stores its layer id and children inherit the nearest ancestor's so
one subtree paints on one layer, paint order buckets the root list by layer (canvas,
hit testing, and SVG, PNG, and PDF export all walk it), and the Layers dialog plus the
MCP layer tools cover add, rename, duplicate, delete with shapes, reorder, hide, show,
lock, move the selection to a layer, and select all on layer, each one undo step.
Round 15 closed M4 shape registry, search, and libraries (see ADR-010): the
Library tab now has fuzzy registry search over 30 definitions in six
categories, a persisted scratchpad of pinned and recent shapes, drag-out
insertion with a ghost preview, and quick add on empty-canvas double-click
under Select. Registry entries are ordinary nodes built by pure geometry
builders over existing kinds and paths, plain entries take the page shape
default on insert and in previews, and insertion is one undoable layer-aware
step that joins the frame under the point or a named parent. Autosize in the
Arrange tab and on Alt+F hugs groups and frames deepest-first without moving
children. MCP gains search_shapes and insert_shape with layer and parent
arguments. Custom shape XML, public galleries, a parametric shape editor, and
a property bag stay deferred.
Round 16 closed M5 draw.io import and export (see ADR-011): the adapter parses
mxfile and mxGraphModel XML with roxmltree, decodes compressed payloads, refuses
encrypted ones, and maps pages, layers, groups, frames, styles, labels, edges,
extra edge labels, and registry shapes to native content with a report of
approximations, drops, and warnings. The app shows the report in a dismissible
window, appends imported pages with their layers to the current project, and
creates a project from the home screen. Export writes every page as one
uncompressed .drawio file from the inspector, the CLI, or the export API. A
checked-in two-page fixture drives unit and integration tests, and geometry
round-trips within 0.01 world units. Pen paths and freehand strokes export as
basic shapes or edges, a documented limitation. Opening the exported file in
draw.io itself was not run in this environment; the reimport test and the
documented mxfile structure support it. IO-17 (VSDX, Gliffy, Lucidchart),
IO-18 (HTML embed), and DOC-15 (embedded diagram data in PNG, SVG, HTML, or
PDF) stayed blocked or deferred at that point. Round 26 later added the PNG and
SVG part of DOC-15.
Round 17 closed M6 diagram data, links, tags, find, and placeholders (see
ADR-013): nodes carry an ordered metadata map, a custom link, a tooltip, and a
tag list, all optional and skipped while empty. The Data tab edits them, the
Edit Data dialog covers shape and page properties, and a transient page context
resolves percent placeholders at paint and export time from shape, ancestor,
and page metadata plus page name, page number, page count, file name, and date
values. A missing value renders a visible fallback, a double percent escapes a
literal percent, and the saved text stays a template. Cmd/Ctrl click and the
Data tab Open Link follow page and external links; page targets switch pages,
and external targets are limited to http, https, and mailto with a confirmation
before the platform opener runs. Find searches labels, connector labels, text
and sticky content, frame names, tags, tooltips, and metadata keys across pages,
with case sensitivity, optional regular expressions, next and previous stepping,
and replace as one undo step per page that skips locked and hidden nodes. The
draw.io adapter reads and writes UserObject attributes and a sketch-data
sidecar, so the metadata fixture round-trips its data.
Round 18 closed M7 templates and diagram packs (see ADR-010): a template
library is a validated JSON file with a format name and version, named
templates carrying a category, description, keywords, and an optional SVG
thumbnail, and pages of ordinary nodes with root order, diagram settings, and
layers. A node may name the registry shape it came from; when the running
build lacks it, insertion reports the shape and skips only that node while
repairing bindings, child references, and instance links. Fifteen bundled
packs cover flowchart, UML class, ER, BPMN, network, floor plan, org chart,
kanban, timeline, Gantt, C4, SysML, charts, Venn, and a three-page project
plan, all pure builders over native kinds and registry shapes with no images,
icons, network requests, or executable content. The browser opens from the
Library tab or the command palette, groups by library, searches and filters
by category, previews a page as vector content, and inserts one page on the
open page or every page as new pages under one undo step, keeping a layer
when its name matches a page layer. A custom folder saved in options loads
every JSON library beside the packs, reports unreadable files, and the open
page saves back as a template with a thumbnail. MCP gains list_templates and
insert_template with page and page-set modes.
Round 19 closed M8 data and text generation (see the ADR-011 extension): the
new generate module turns local text into an ordinary document that insertion
adds through the same subtree path as libraries and templates. Mermaid
flowcharts and sequence diagrams become nodes, shapes, and bound connectors
with subgraphs, labelled edges, self-loops, participants, lifelines, and
messages; unsupported directives are reported as warnings rather than
failures. CSV detects the delimiter, honours quoted fields, and lays rows out
as a grid, a vertical or horizontal flow, or a vertical or horizontal tree
with an optional header and a one-based label column. SQL CREATE TABLE and
ALTER TABLE statements become table frames with column rows, primary and
foreign key markers, and foreign-key connectors. JSON graph input accepts
nodes and edges with from or source and to or target, keeps coordinates only
when every node has them, and otherwise lays the graph out deterministically,
warning about dangling edges. The Insert menu, the command palette, and the
MCP generate_diagram tool share one dialog and one parser set; a diagram
lands on the open page as one undo step or as a new page. AI generation
(GEN-04) stays blocked, as does GEN-10 vendor stencil licensing.
Round 20 closed UIX-11, the search omnibox (see the ADR-010 and ADR-013
extensions): the Ctrl+K command palette now searches registry shapes and the
text inside the diagram from the same field. Shape entries carry the category
and insert an ordinary node at the viewport center in one undo step. Text
entries reuse the find engine's fields, matching rules, labels, and previews,
switch to the owning page, select the node, and zoom to it. Text hits cap at 40,
dedupe per node field, and appear only for a non-empty query, so opening the
palette does not list every label. Commands, tools, pages, layers, templates,
generators, and library items stay in the same ranked list.
Round 21 closed CAN-15, fixed connection points: the eight anchors, four
corners and four edge midpoints, are a pure helper over a world box, and the
overlay draws them as small crosses on the hovered bindable shape or on the
shape a connector end is being dragged over while the page's connection points
setting is on. A dropped end within 12 screen pixels of an anchor stores that
anchor's normalized fraction through one wrapper shared by both end drags and
new arrows, so the end keeps the same corner or midpoint when the shape moves
or resizes. The old four-circle candidate overlay is gone.
Round 22 closed CAN-16, rulers and manual guides: a page stores a Rulers flag
and a list of manual guides, each an axis plus a world position. The Diagram
tab gains a Rulers checkbox and a Clear Guides button. Rulers draw as 18 px
strips along the canvas top and left with 1-2-5 tick spacing and labels;
dragging out of a ruler creates a guide, dragging the line moves it, dropping
it on the matching ruler deletes it, and each gesture is one undo step, with a
drop on the ruler recorded as Delete guide. Committed guides outrank center
alignment, equal spacing, and grid snapping for moves, and a resize snaps only
the dragged edge. Guides paint as blue lines in the canvas overlay and the
whole set persists with the page.
Round 23 closed TXT-20, superscript and subscript: a text style and each text span
carry a script value of normal, super, or sub. The Text tab gains x² and x₂ toggles
after S; with no text selection they set the node's base, and inside a text selection
they override the selected characters. Applying one script replaces the other, and
pressing the active toggle returns the range to normal. Superscript and subscript
glyphs draw at 0.7 of the base size with baseline shifts of -0.35 em and 0.15 em;
line boxes keep the base size, rich and bidirectional layout share the metrics, clear
formatting resets the base, and the page file round-trips the base plus per-span
values.
Round 24 closed SHP-19, rotation completion: a node stores a clockwise angle and a
Keep label upright flag. A circular handle above the selection's top-right corner turns
an unlocked shape or empty frame in 15 degree steps, or 1 degree with Shift; the
Arrange tab adds an exact Angle field and a Rotate shape only by 90 degrees button.
Dragging and the quarter-turn button turn the label with the shape unless the flag is
set, while the exact field follows the flag. Rotated shapes hit test through their
transform, the overlay draws the handle, and both values join undo and redo, survive
save and reload, and reach PNG, SVG, and draw.io export. Groups, frames with children,
and connectors have no handle, and draw.io export has no upright-label style.
Round 25 closed CON-19, per-shape default connection style: a non-group shape can
store the style for connectors started from it. The Style tab copies a selected
connector's style to the shape its line starts from, and the Arrange tab can copy
the page connector default to selected shapes or clear their source default. Each
command is one undo step. New arrows, including quick-connect arrows, use the
source default before the page default; existing connectors keep their own style.
The value travels in the page file and in the draw.io UserObject sidecar (ADR-008).
Round 26 closed the PNG and SVG part of DOC-15, embedded recoverable diagram data.
An opt-in Export tab checkbox and a CLI flag add a versioned native bundle to those
files. The bundle carries the project name, active page id, every page and camera,
and the bytes of image assets the pages use. SVG stores base64 text in a metadata
element with id sketch-rs-data; PNG stores it in a tEXt chunk. Import checks that
carrier first and rebuilds native pages instead of tracing the rendered artwork.
Files exported without the option keep their existing paths. PDF, HTML, and
draw.io-native metadata remain absent.
Round 27 fixed a user-reported dark-theme bug. Connector lines, arrowheads, solid heads, and
transparent connector labels now lift to at least 4.5:1 contrast against the dark canvas at
render time. The mapping keeps the authored alpha and hue. White painted pages keep the
stored colors in dark theme, and a theme change rebuilds cached pieces. Saved documents and
PNG, SVG, and PDF exports keep the authored colors.
Round 28 closed CON-16, flow animation for connectors. Each node carries an optional flow
setting with enabled, duration, timing, and direction. The renderer advances a dash offset
from the editor clock, and flowing connectors rebuild their meshes each frame instead of
coming from the scene cache. The Arrow section in the inspector edits all four values in
one undoable step. draw.io import and export map flowAnimation, flowAnimationDuration,
flowAnimationTimingFunction, and flowAnimationDirection, and import still reads the older
flowDuration key. Exports and saved documents keep authored dash values; only the
interactive canvas animates.

## Verified this round

- The failure came first. `flow_dashes_move_with_time` produced identical mesh
  vertices at 0 ms and 250 ms before the dash offset was wired into tessellation.
  The log is `.artifacts/connector-flow/failure.log`.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `git diff --check`: clean. `cargo build --release`: OK.
- `cargo test`: 427 tests pass (338 lib + 89 integration). Eleven new
  `tests/flow.rs` tests cover motion, stable output when disabled, export-time
  dash state, old JSON compatibility, timing and direction math, scene cache
  rebuilding, undo and redo, draw.io round trips, legacy duration import, native
  JSON persistence, and rendered frame differences.
- Headless GPU frames from the interactive canvas renderer used a real flowing
  connector. The 0 ms and 225 ms frames differ in 3286.85 pixels; the same pair
  with flow disabled differs in 0 pixels. Evidence is in
  `.artifacts/connector-flow/frames/`, with `frame-evidence.log` and the pair and
  diff images.
- Docs checker: `python3 .artifacts/check_parity_docs.py` passed with 0 failures;
  output copied to `.artifacts/connector-flow/round28-docs-check.txt`.
- `docs/screenshots` restored after the final test run.

Round 29 closed UIX-04, collapsible and searchable format sections. Option files
now carry a compact map of collapsed section keys, with missing keys defaulting
to expanded. The format panel has a session-only filter field: a query keeps
matching sections open, hides nonmatching rows, and restores saved collapse
state when cleared. A no-match query reports that no properties matched; this
required replacing an early return in the global-tab branch so the message is
reachable when nothing is selected. Six focused inspector tests and the options
round-trip test cover the behavior. The first options test failed because the
saved JSON omitted the map; its failure is kept in
`.artifacts/round29/failure.log`. Runtime on Xvfb `:99` used a release build and
a temporary store root so user options were untouched: the filter field accepted
queries, a chevron wrote a collapsed key, the key survived a restart, and one
click after restart expanded the section again. Screenshots, clipboard readbacks,
options snapshots, and a recorded session are in `.artifacts/round29/`.

An accidental whole-file overwrite hit `src/ui/inspector.rs` during this round.
The exact pre-overwrite source was recovered from snapshot blob
`069a40b62c1ee4fc0acf103b99364f70191e3726` and passed `cargo check` before any
test code was added. Use targeted edits for that file, never a whole-file write.

Round 30 closed CON-17, fixed versus floating connector endpoints. A binding
now carries a `floating` flag that defaults to false, so saved documents keep
the old fixed behavior. A floating end focuses the bound shape's rect center
while a fixed end keeps its normalized point. draw.io import treats a missing
exit or entry coordinate as floating, and export writes those keys only for
fixed ends. The Arrow section has Start and End rows with Fixed and Floating
buttons; switching back to fixed preserves the current point, and the whole
change is one undo step. Four tests cover the panel, geometry, import and
export, and undo/redo. The first failure had no endpoint controls; live Xvfb
evidence later recorded five passing assertions with saved state snapshots and
a verified 1366x768 video in `.artifacts/round30/`.

Round 31 closed CON-13, connector head size and offset. A node now carries
defaulted start and end head sizes and offsets, with draw.io's marker size 6 as
the neutral value. Head geometry scales around that value and positive
perimeter spacing moves each tip into the line, so old documents keep their
current rendering. draw.io import and export map startSize, endSize,
sourcePerimeterSpacing, and targetPerimeterSpacing in both directions and write
only non-default values. The Arrow section exposes head size and head offset
with separate start and end fields, and each edit joins the coalesced undo and
redo path. Seven tests cover geometry, JSON defaults and non-defaults, draw.io
round trips, panel text, and undo and redo. The first panel test failed before
the controls existed; a focused offset test also caught and fixed a reversed
start sign. Live Xvfb evidence recorded six passing assertions, saved seven
state snapshots, measured canvas changes, and exported all four style keys in
`.artifacts/round31/`.

Round 32 closed STY-22, adaptive colors for light and dark mode. A page stores an
adaptive mode of auto, simple, or none. Round 32 first described Auto as
computing a dark variant for a fixed color. Round 34 corrected that behavior: Auto
keeps authored colors and explicit dark pairs, Simple flips only neutral black
and white, and None draws the page light. Light values stay authoritative in the
saved document. Fills, strokes, gradient stops, text styles, labels, and page
backgrounds can carry optional dark values that resolve at the document boundary
through `Document::with_appearance`. The renderer and its cache use the resolved
appearance, draw.io import and export map `light-dark()` and `adaptiveColors`,
PNG and SVG exports bake a requested appearance, and the CLI accepts
`--appearance light|dark`. The Diagram tab edits the mode, dark page background,
and dark color values through real state and undo. Dark-theme text stays readable
because the renderer lifts low-contrast glyphs without rewriting shape fills.
Focused tests, live Xvfb evidence with three passing assertions, and light and
dark export pairs are in `.artifacts/round32/`. Nothing committed or pushed.

Round 33 closed STY-21, global style schemes. Ten named schemes are shared by the
global Style tab and the selected-element Style tab, because the global tab is not
reachable while elements are selected. Page scope restyles eligible elements and sets
the page scheme, background, and grid. Selection scope restyles only selected
subtrees and leaves page settings untouched. A SchemeReport previews and then reports
changed, locked, hidden, out-of-scope, and unstyleable elements; locked, hidden, and
group nodes are skipped. The restyle keeps rich text, metadata, component instance
overrides, per-corner radii, and adaptive color pairs. apply_scheme_scoped joins
undo and redo as one edit, and the page file stores the scheme. draw.io export writes
sketchScheme on mxGraphModel and import validates the name against SCHEMES. The first
failure showed the old apply_scheme restyling locked nodes and dropping dark values;
it is in `.artifacts/round33/failure.log`. Focused editor, draw.io, and inspector
tests pass. The scheme cards overlapped because egui Grid reported zero minimum width
for the custom-painted cards; explicit horizontal rows fixed it, with before and
after rectangle logs in `.artifacts/round33/`. Live Xvfb `:101` evidence includes a
verified 532.77 s video, five passing assertions, state checks for both scopes and
undo/redo, and a CLI export containing sketchScheme="Classic". Final checks and the
docs check are recorded in `.artifacts/round33/`. Nothing committed or pushed.

Round 34 closed STY-13 and DOC-13, shape shadow and the page-wide Shadow
setting. A page stores `settings.shadow`; a node stores an optional
`style.shadow`, so an explicit per-element value overrides the page value. Draw
ops put a translucent copy of the fill and stroke at (3, 4). SVG export bounds
and scene culling use `draw_bounds_with_shadow`, while hit testing uses the
no-shadow bounds. draw.io import and export map `shadow=1` and `shadow=0`, and
the Diagram and Style tabs edit both values through undo. The first failure
showed an absent shadow in the SVG bounds; it is in `.artifacts/round34/failure.log`.

The user-reported color regression failed first: a green fill resolved to blue
in the dark theme. Auto now keeps authored colors and explicit dark variants.
Scheme restyling no longer generates complements from saved dark values. Text
visibility moved into the renderer: it lifts low-contrast glyphs against the
actual text background or dark canvas and leaves text-shadow glyphs alone. The
failures and fixed runs are in `.artifacts/round34/`. Focused shadow tests, 368
lib tests, and the skip run of all test binaries pass. The verified 168.9 s live
video has two passing assertions in
`.artifacts/round34/live2/evidence-20261118-225810-1dbabe/report.md`. Dark
canvas pixels kept the green, orange, and white fills, and the SVG viewBox grew
from `142x92` to `145x96` with page shadow on. Nothing committed or pushed.

Round 35 closed STY-14, the glass effect. A node stores `style.glass` as a
defaulted bool that native JSON omits when false. The renderer paints a
three-stop white gradient over the fill after the shadow and below the label,
using the shape's own path so rounded corners follow it. The Style tab Effects
section exposes Glass beside Shadow and edits the flag through the same
undoable apply path. draw.io import reads `glass=1` and `glass=0`, while export
writes only `glass=1` because false is the default. The first draw.io test
failed before the mapping; the log is in `.artifacts/round35/failure.log`.
Five focused tests pass, including native JSON, draw ops, panel text and
undo/redo. The final skip run passes 473 tests. Live Xvfb `:103` evidence has
a verified 89.87 s video with four passing assertions. Clicking the checkbox
removed the canvas gradient, undo restored it, PNG export kept it, and a CLI
draw.io export and reimport carried the flag. Nothing committed or pushed.

Round 36 closed STY-23 and fixed the dark-canvas contrast report. The Style tab
has a typed property registry with draw.io-like keys, a local filter, real
edits, validation, undo and connector-specific rows. The renderer now lifts
low-contrast glyphs on dark pages to 14:1, so GPU arrow pixels rose from 133 to
235 while light exports kept authored colors. The first panel test and the
first contrast test failed before their fixes. Focused tests, final checks and
frames are in `.artifacts/round36/` and `.artifacts/contrast/`. Implementation
was pushed as 7261dfd; the parity docs now record VERIFIED 113 and DISCOVERED
53.

Round 37 closed UIX-14, the first-run tour. A fresh store opens a three-step
welcome panel covering shapes, connectors and export. Dismissal persists in
options, and Help reopens the panel. The first tests failed because no onboarding
flag or welcome state existed. The first live recording hit Skip and was
discarded; the corrected Xvfb :104 run verified step advance and the saved
onboarding_seen flag in an 82 s video. Implementation was pushed as 858d1a0; the
parity docs now record VERIFIED 114 and DISCOVERED 52.

Round 38 closed TXT-21, lists and indentation. `TextStyle.paragraphs` carries a
list kind and indent level for each hard-break paragraph. Marker glyphs and
numbering render with indent, list buttons apply to all paragraphs in one undo
step, and Indent and Outdent affect the active paragraph. Native JSON round-trips
the values while ordinary text stays unchanged. Six focused tests pass. The first
failure had no list metadata or Text tab controls; it is in
`.artifacts/round38/failure.log`. The parallel skip run hit an intermittent mcp
SIGSEGV twice, so the final run used one test thread and passed 486 tests. fmt,
clippy, diff-check and the release build pass. A verified 778 s Xvfb `:104`
recording with passing assertions is in
`.artifacts/round38/evidence/evidence-20261119-120150-d532e5/report.md`. Debug
instrumentation was removed before the last release build; screenshots were
restored after test runs. Both commits were pushed; the parity docs recorded
VERIFIED 115 and DISCOVERED 51 at the end of the round.

Round 39 closed IO-19, print. Ctrl/Cmd+P opens a Print dialog with paper,
orientation, fit or custom scale, margin, and page or selection scope.
`io::pdf` writes a paper-sized MediaBox and centers the scaled content inside
the margins. The primary Print button writes to the project folder (or a temp
folder when no project is open) so printing is deterministic under bare Xvfb;
Choose location keeps the rfd save dialog. The first failure had no print API
and is in `.artifacts/round39/failure.log`. Eight focused and integration tests
pass; the serial skip suite passes 495 tests with the known
`m2_pages_panel_screenshot` failure skipped. fmt, clippy, diff-check and release
build pass. A verified 490.87 s Xvfb `:105` recording with four passing
assertions is in
`.artifacts/round39/evidence/evidence-20261119-124018-fd7149/report.md`, with the
printed Letter PDF saved beside it. The dialog position shifts between opens,
so live clicks used a fresh open each time; fixed pixel coordinates are not
reliable across opens. Both commits were pushed; the parity docs recorded
VERIFIED 116 and DISCOVERED 50 at the end of the round.

Round 40 closed DOC-09, the per-page background image. A page stores an image
asset id, fit mode and opacity; the registry owns the bytes by content hash, and
save, embedded bundles and reload carry them. The renderer paints the image in
screen space behind the grid and content, and PNG, SVG and PDF exports include
it for whole-page scopes only. The Background dialog gained Choose image, Clear,
fit mode and opacity controls. The first failure had no background image model
and is in `.artifacts/round40/failure.log`. Four integration tests and five lib
tests pass; the serial skip suite passes 503 tests with the known
`m2_pages_panel_screenshot` failure skipped. fmt, clippy, diff-check and release
build pass. A verified 37.10 s Xvfb `:105` recording with five passing
assertions is in
`.artifacts/round40/evidence/evidence-20261119-132330-5a28e0/report.md`. The
image had to be seeded into a scratch project because native rfd dialogs do not
appear under bare Xvfb. A fresh Xvfb display mapped no usable app window, so
the working display and `xdotool windowfocus` were reused. Both commits were
pushed; the parity docs recorded VERIFIED 117 and DISCOVERED 49 at the end of
the round.

Round 41 closed TXT-24, curved text. `TextStyle.curve` stores a fixed arc height
as a fraction of the font size, and `apply_curve` lifts glyph baselines along a
quadratic arc while keeping glyphs upright. The Text tab gained an ARC toggle,
draw.io style maps `textPath=1;` in both directions, and MCP `update_styles`
accepts `textPath`. TXT-19 was checked first and rejected: `WritingDir::Vertical`
and `layout_vertical` already implement vertical text at HEAD. The first failure
is in `.artifacts/round41/failure.log`. Four focused tests pass; both the first
full run and the final run after the MCP change pass 507 tests with the known
`m2_pages_panel_screenshot` failure skipped. fmt, clippy, diff-check and release
build pass. A verified 38.60 s Xvfb `:105` recording with four passing
assertions is in
`.artifacts/round41/evidence/evidence-20261119-134656-0f3f64/report.md`. The ARC
button was not clicked live because its position is unreliable under bare Xvfb;
Text Path was toggled through MCP. Native and draw.io round trips are covered by
tests, not a live save and reopen. Both commits were pushed; the parity docs
recorded VERIFIED 118 and DISCOVERED 48 at the end of the round.

Round 42 closed CAN-06, custom grid color. The page setting, renderer and
Diagram tab control already existed. Draw.io has no standard page grid-color
attribute, so export now writes a `__sketchGridColor` sidecar on `mxGraphModel`
and import restores it. The tooltip no longer says the color is not exported.
Three focused tests cover native JSON, the draw.io round trip and a GPU frame
whose grid changes color. The serial skip suite passes 510 tests with the known
`m2_pages_panel_screenshot` failure skipped. fmt, clippy, diff-check and release
build pass. A verified 42.77 s Xvfb `:105` recording with four passing
assertions is in
`.artifacts/round42/evidence/evidence-20261119-135940-3eab09/report.md`. The
color was seeded into page JSON and the app restarted between states because the
native picker is not driven under bare Xvfb. The reimport half is covered by a
test, not the video. Both commits were pushed; the parity docs recorded
VERIFIED 119 and DISCOVERED 47 at the end of the round.

Round 43 closed DIA-10, page animations. A page now stores a fade transition
with a duration clamped to 100 to 3000 ms, edited on the Diagram tab. Switching
pages arms the fade on the next tick, so an idle editor clock cannot finish the
first frame at once; a translucent veil fades out over the new page. Draw.io has
no page-transition attribute, so export writes `__sketchPageTransition` and
`__sketchPageTransitionMs` sidecars that draw.io ignores and Sketch restores on
import. Five focused tests cover model, timing, stale clock, a GPU frame and
draw.io round trips; one inspector test covers the panel controls. The first
live run exposed the stale-clock bug and is saved. The corrected serial skip
suite passes 516 tests with the known `m2_pages_panel_screenshot` failure
skipped. fmt, clippy, diff-check and release build pass. A verified 31.60 s Xvfb
`:105` recording with four passing assertions is in
`.artifacts/round43/evidence/evidence-20261119-142023-9dad02/report.md`: the red
page baseline, a mid-veil center of (235, 235, 237) against a settled
(34, 80, 217), the blue page after 3 s, and the draw.io sidecar with 3000 ms.
The two-page project was seeded in JSON and the Diagram tab checkbox is covered
by a unit test rather than a live click. All three commits were pushed; the
parity docs recorded VERIFIED 120 and DISCOVERED 46 at the end of the round.
DIA-11 animation actions stay blocked on a sandboxed action interpreter.

Round 44 closed CUS-05, custom shape library XML. A new adapter reads the JSON
array inside `<mxlibrary>`, passes each entry's uncompressed mxGraphModel
through `drawio::import_str` and writes entries through `drawio::export`.
Imported shapes become personal library items and insert through the existing
`insert_library_item` path. The Library tab gained Import XML and Export XML
buttons backed by tested file helpers, and the CLI gained `--import-lib` and
`--export-lib`. The first focused run found that draw.io shorthand styles such
as `rhombus;` imported as rectangles; `shape_name` now accepts those tokens.
Six focused tests pass, and the serial skip suite passes 522 tests with the
known `m2_pages_panel_screenshot` failure skipped. fmt, clippy, diff-check and
release build pass. A verified 38.53 s Xvfb `:105` recording with three passing
assertions is in
`.artifacts/round44/evidence/evidence-20261119-144811-d04937/report.md`: the
imported Decision tile renders its #dae8fc fill, clicking it inserts 1602 blue
pixels on the canvas, and the export writes both shapes with escaped models.
The import used the CLI before recording because bare Xvfb cannot drive the
native picker; the panel file operations are test-covered. Both commits were
pushed; the parity docs recorded VERIFIED 121, ANALYZED 5 and DISCOVERED 46 at
closeout. Compressed or encrypted library entries stay
unsupported with a clear error.

Round 45 closed TXT-06, the font family picker. The inspector already listed
installed families through `text::font_families()`, but MCP still reported only
Inter and CSS and draw.io import dropped a chosen family. `text::family_info()`
now reports availability and the discovered faces, with Inter bundled in four
styles. MCP get_font_family_info uses it, get_basic_info lists discovered
families, CSS `fontFamily` writes `text_style.family`, and draw.io maps it on
shape labels as well as text nodes. The first failure showed Adwaita Mono
unavailable with no styles. Five focused integration tests and one unit test
pass; the serial skip suite passes 528 tests with the known
`m2_pages_panel_screenshot` failure skipped, and screenshots were restored.
fmt, clippy, diff-check and release build pass. A verified 83.03 s Xvfb `:105`
recording with three passing assertions is in
`.artifacts/round45/evidence/evidence-20261119-150844-eb3842/report.md`: MCP
reports Adwaita Mono available with four styles, get_basic_info lists 381
families against the earlier single Inter entry from round 34, and a
font-family HTML round trip reports the chosen family while adding 1604 dark
pixels. The calls went over HTTP rather than the inspector, the flow polled
until the background font scan landed, and the code was committed after the
recording so the report names the earlier commit. Both commits were pushed; the
parity docs recorded VERIFIED 122, ANALYZED 5 and DISCOVERED 45 at closeout.

Round 46 closed CON-18, reverse connector. The row was stale: the Arrange tab
Reverse button, `Editor::reverse_connectors` and two editor tests were already
at HEAD. The missing piece was MCP, so `reverse_connectors` now takes `nodeIds`,
resolves each on its page, reverses points and swaps bindings in one undo step
per page, and reports `reversed`, `count`, `notConnectors` and `notFound`. Heads
stay on the line's own ends and move to the opposite shape, matching draw.io's
`turnShapes` edge branch. The first failure was `unknown tool
'reverse_connectors'`. A first focused run asserted hard-coded points and failed
because insertion reroutes the arrow; the tests now compare with points captured
before the call. Two integration tests pass; the serial skip suite passes 530
tests with the known `m2_pages_panel_screenshot` failure skipped, and screenshots
were restored. fmt, clippy, diff-check and release build pass. A verified 33.73 s
Xvfb `:105` recording with three passing assertions is in
`.artifacts/round46/evidence/evidence-20261119-152715-46ade4/report.md`: the
saved page JSON swaps start 1 / end 2 to start 2 / end 1 with reversed points
and an unchanged triangle head, the reverse changes 240 canvas pixels, and one
undo restores the original JSON with 0 canvas pixels changed. The calls went
over HTTP rather than clicking the inspector button, bindings came from the
saved page JSON because `get_node_info` omits connector ends, and the pixel diff
excluded status-bar text so the undo result was not masked. The code was
committed after the recording, so the report names `unknown`. Implementation
committed as 221cf0f; both commits were pushed and parity docs recorded VERIFIED
123, ANALYZED 5 and DISCOVERED 44 at closeout.

Round 47 closed DIA-05, tags. The row was stale: `Node.tags`, the Data tab
editor, the find bar's tag search and draw.io's tags attribute were already at
HEAD. `find_nodes` in MCP had no tag query, so a tag-only search was rejected
and a combined query ignored the tag. It now accepts `tag` with the same
case-insensitive glob as the editor find bar, reports the stored tag in matched,
combines with text and style filters using AND, and honours page and subtree
scope. The first failure was `pass textValue and/or filters`. A first focused
run expected a lowercase match for a node tagged `Web`; the tool correctly
reports the stored value. Four integration tests pass; the serial skip suite
passes 534 tests with the known `m2_pages_panel_screenshot` failure skipped, and
screenshots were restored. fmt, clippy, diff-check and release build pass. A
verified 30.27 s Xvfb `:105` recording with three passing assertions is in
`.artifacts/round47/evidence/evidence-20261119-154205-45489a/report.md`: seeded
tags load through `get_node_info`, `tag: web` finds two shapes and `tag: *API*`
finds one, and `select_nodes` selects both matches and changes 4897 canvas
pixels. The calls went over HTTP rather than clicking the editor find bar, and
the code was committed after the recording so the report names `unknown`.
Implementation committed as 2ec2fbd; all three commits were pushed and parity
docs recorded VERIFIED 124, ANALYZED 5 and DISCOVERED 43 at closeout.

Round 48 closed DIA-04, tooltips. The data side was already at HEAD: the node
field, Data tab editor, find support, draw.io mapping and `get_node_info`
reporting. The unmet acceptance criterion was that the tooltip "truncates
cleanly". The first failure showed a 600-character tooltip at full length.
`tooltip_display_text` now clips resolved text to 200 characters on a character
boundary with a trailing ellipsis, wraps at 260 px and constrains the popup to
the canvas. Two unit tests pass; the serial skip suite passes 536 tests with the
known `m2_pages_panel_screenshot` failure skipped, and screenshots were restored.
fmt, clippy, diff-check and release build pass. A verified 20.00 s Xvfb `:105`
recording with three passing assertions is in
`.artifacts/round48/evidence/evidence-20261119-160259-ac7a91/report.md`: real
`xdotool` pointer moves hover a seeded 1024-character tooltip, the popup is
bounded at 261x84 px inside the canvas and clears when the pointer leaves. The
ellipsis is checked by the unit test, not OCR. Two earlier recordings were
stopped and replaced. The code was committed after the recording so the report
names `unknown`. Implementation committed as cca0145; both commits were pushed
and parity docs recorded VERIFIED 125, ANALYZED 5 and DISCOVERED 42 at closeout.

The user then reported overlapping welcome text and asked for a product-focused
README. The dialog passed 1.45 to egui as a line height, but egui reads it as
points. The body galley was 2 px tall, so both lines drew on top of each other.
The line height is now 18.0, and `welcome_body` owns the body text. The focused
test builds a galley and asserts a height of at least 30 px. It fails with the
old value and passes at 36 px with the fix. Evidence is in
`.artifacts/welcome-fix/`. The README now leads with product value, use cases,
file ownership and shortcuts. Developer details stay at the bottom. Links and
the Rust 1.95 requirement were checked. fmt, clippy, release build, diff-check,
the parity docs check and 537 serial tests pass with the known screenshot test
skipped, and screenshots were restored. All commits were pushed.

Round 49 closed DIA-03, custom links. The inventory row was stale. Node links,
Cmd/Ctrl click, the Data tab Open Link button, page targets, the http, https and
mailto allowlist with confirmation, draw.io mapping and `get_node_info`
reporting were already at HEAD. MCP could read links but not edit them. The
first failure in `.artifacts/round49/failure.log` showed four new tests failing
with unknown tool set_link. `set_link` now takes a batch of node ids and link
targets, clears a link on an empty string, rejects unsafe targets through
`crate::link::classify`, and records each call as one undo step. Four
integration tests pass. The serial skip suite passes 541 tests with the known
`m2_pages_panel_screenshot` failure skipped, and screenshots were restored. fmt,
clippy, diff-check and release build pass. A verified 37.53 s Xvfb `:105`
recording with five passing assertions is in
`.artifacts/round49/evidence/evidence-20261119-174935-45d106/report.md`: a
seeded shape starts unlinked, one call stores an external and a page link,
`get_node_info` reports both, a javascript target is refused without changing
the stored link, the page file keeps both links after finish, and both shapes
can be selected on the canvas. The first recording was discarded because its
last frame went black. Calls went over HTTP rather than the Data tab, and the
page target was stored but not opened in a viewer. The report names baseline
`fce3746` because the implementation was committed as ea87c41 after the
recording. Code and docs commits (`ea87c41`, `e325a8b`) were pushed. Parity
docs record VERIFIED 126, ANALYZED 5 and DISCOVERED 41 at closeout.

Round 50 closed SHP-19's draw.io upright-label gap. Native rotation and
`label_upright` were already at HEAD, but draw.io export dropped the upright
flag and import never read it back. The first failure in
`.artifacts/round50/failure.log` came from
`export_and_reimport_keep_an_upright_label_flag`. draw.io export now writes a
native `__sketchLabelUpright` value in `sketch-data`, and import consumes it so
the key does not stay in node metadata. The draw.io test suite passes (11
tests). The serial skip suite passes 542 tests with the known
`m2_pages_panel_screenshot` failure skipped, and screenshots were restored. fmt,
clippy, diff-check and release build pass. A CLI export of a seeded rotated
upright-labeled shape produced `.artifacts/round50/io/upright.drawio` with
`rotation=30` and the sidecar, and reimporting it produced
`.artifacts/round50/io/imported-node.json` with rotation 30 and `label_upright`
true. The implementation and docs commits (`30636ea`, `b7ce024`) were pushed.

Round 51 was committed as `4e5b64d` and pushed: stale rows DIA-02, DIA-06,
CUS-16, DIA-01, SHP-17, UIX-04 and UIX-11 closed as evidence-only doc updates,
and DIA-07 replace-current-match was implemented and verified with focused
checks.

Round 52 was committed as `5ffd939` and pushed: stale CON/CUS/GEN/TXT rows were
closed as evidence-only doc updates.

Round 54 was committed as `133f255` and pushed: stale placeholder and diagram
property rows were closed as evidence-only doc updates.

Round 55 was committed as `5e08e52` and pushed: stale fixed connection point and
manual guide rows were closed as evidence-only doc updates.

Round 56 was committed as `3436e65` and pushed: stale draw.io import/export/loss
reporting and endpoint-mode rows were closed as evidence-only doc updates.

Round 57 was committed as `15863fe` and pushed: stale head-style and
generation rows were closed as evidence-only doc updates.

Round 58 was committed as `6080801` and pushed: stale template library rows
were closed as evidence-only doc updates.

Round 59 was committed as `b2380a6` and pushed: stale vertical text row was
closed with a focused layout test and parity doc updates.

Round 60 was committed as `8f85118` and pushed: stale diagram-pack rows were
closed as evidence-only doc updates with partial-acceptance limitations.

Round 61 was committed as `8734350` and pushed: stale blocked-row wording was
cleaned to reflect committed HEAD state.

Round 62 was committed as `7a81b40` and pushed: stale DISCOVERED row CUS-11 was
closed as an evidence-only doc update against HEAD shape metadata/property
editing and existing tests.

Round 63 was committed as `d2fe51c` and pushed: stale ANALYZED/BLOCKED rows
ORG-13, ORG-14 and ORG-15 were closed as evidence-only doc updates against HEAD
page-layer behavior and existing tests.

Round 64 was committed as `f43aca4` and pushed: stale IN_PROGRESS rows DOC-12
and UIX-03 were closed as evidence-only doc updates against HEAD per-page grid
settings and Diagram-tab behavior (DOC-12 IMPLEMENTED, UIX-03 VERIFIED) and
UIX-02 limitations were refreshed while kept IN_PROGRESS.

Round 65 was committed as `dedcd0a` and pushed: added integration test
two_pages_keep_different_grid_settings_across_switch_save_and_reload in
tests/projects.rs proving two pages keep different grid visibility, size, and
color across page switches, save, and reload. DOC-12 moved from IMPLEMENTED to
VERIFIED.

Round 66 was committed as `cb6a98c` and pushed: stale IMPLEMENTED row UIX-01
was closed as an evidence-only doc update against HEAD tabbed-inspector
behavior and existing tests (UIX-01 VERIFIED).

Round 67 was committed as `a187293` and pushed: advanced UIX-02 with
selection-aware element tab computation in `src/ui/inspector.rs` and
`src/ui/mod.rs`; Text tab omitted when no selected element carries editable
text or a label and active tab clamped to current selection context.

Round 68 was committed as `d23302f` and pushed: Style-tab connector/sticky/
sketch sections resolve from any selected element, not only the first selected
node.

Round 69 (local, pending commit/push) advanced UIX-02: unlabeled connector-only
selections now omit the Text tab (Style, Arrange, Data) while text layers keep
Style, Text, Arrange, Data, giving different tab sets for the typical connector
vs text-layer case. Unlabeled single connectors show an Add label button in the
Style tab Arrow section; labeled connectors still include Text. Added/updated
tests in src/ui/mod.rs and src/ui/inspector.rs. UIX-02 remains IN_PROGRESS
because a labeled connector shares the same top-level tab set as a text layer
and some irrelevant controls/sections may remain in non-Style tabs. Parity docs
record VERIFIED 173, IMPLEMENTED 29, IN_PROGRESS 1, ANALYZED 0, DISCOVERED 1 and
BLOCKED 27. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
targeted lib tests, `git diff --check`, and `python3 .artifacts/check_parity_docs.py`
passed before commit. Full `cargo test` and release build were not rerun.

## Blockers

- No blockers. M9 collaboration remains deferred pending a server, a conflict
  model, and permissions. GEN-04 AI generation and GEN-10 vendor stencils remain
  blocked. IO-17, IO-18 and IO-20 remain blocked or deferred, as do the HTML
  and PDF parts of DOC-15.
- Stale installed MCP binary at `~/.cargo/bin/sketch-rs-mcp` lacks the most
  recent tools in this tree. Run `cargo install --path . --bin sketch-rs-mcp`
  to expose them outside `cargo run --bin`. Token-guarded HTTP/MCP endpoints on
  port 3333 are the reliable path for runtime verification (see
  `.opencode/MEMORY.md`).

## Exact next task

Commit and `git push --quiet` the round 69 unlabeled-connector tab
differentiation progress (code + tests + parity/docs updates) on
`agent/drawio-features`, keeping checks passing. After pushing, continue
assessing UIX-02 (still IN_PROGRESS): typical unlabeled connector vs text-layer
selections now get different tab sets, but a labeled connector still shares the
same tab set as a text layer and some irrelevant controls/sections may remain in
non-Style tabs. Capture a failing test before real code fixes. Assess UIX-12 for
any genuine HEAD evidence (remains DISCOVERED/unaudited). Otherwise close or
correct stale rows as evidence-only with cited HEAD identifiers and update
verification/session docs. Do not restart completed rounds. Keep blocked/
deferred items blocked. No release, tag, or PR.

## Commands

- Checks: `cargo fmt --check && cargo clippy --all-targets -- -D warnings &&
  cargo test && git diff --check && cargo build --release`
- `cargo test` rewrites `docs/screenshots/`; restore with
  `git checkout -- docs/screenshots`.
- Docs checker: `python3 .artifacts/check_parity_docs.py`. It resolves
  backticked identifiers in `feature-inventory.md` against HEAD, so working-tree
  field names must not be backticked there.
- Runtime: `Xvfb :99 -screen 0 1366x768x24 -nolisten tcp -ac`, then
  `env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET SKETCH_RS_MCP_PORT=29981
  DISPLAY=:99 ./target/release/sketch-rs <project>`; confirm `using X11
  backend` in the log. MCP helper `/tmp/opencode/mcp.py` (streamable HTTP at
  `/mcp`, session id header, keep-alive SSE frames must be skipped). Kill test
  apps by exact PID (`pgrep -x sketch-rs`), never `pkill -f`. Launch
  long-lived processes with the harness background flag, not `&`.
- xdotool on `:99` drives real input; clicks need 0.7 s+ spacing or they pair
  into double-clicks (real double-clicks need
  `xdotool click --repeat 2 --delay 90 1`), and MCP state reads lag UI actions
  by seconds, so poll until stable or read the saved project file for positions.
  Panel buttons shift when the panel scrolls; re-screenshot before clicking.
- For dark strokes on the dark canvas, the MCP `get_screenshot` tool renders a
  node or whole page to a light-background PNG and returns it as image content;
  prefer it over dark-canvas screenshots for line and route evidence.

# draw.io parity roadmap

This roadmap sequences the gap between Sketch at commit `5de2f5b` and the draw.io
behaviors recorded in `feature-inventory.md`. It is deliberately narrower than
draw.io. Sketch is a native, local-first canvas, not a browser diagram editor, and
several draw.io capabilities depend on services or assets that do not belong in this
repository.

The current milestone is **M1: draw.io-style format sidebar integration**. Every later
milestone assumes that panel because it is where page settings, connector labels, layer
controls, shape properties, and import options will live. Building those features as
one-off inspector rows first would create a second information architecture that has to
be removed later.

The working tree on `agent/drawio-features` already contains the M1 shell and page
settings, but the work is uncommitted. The inventory records it in the working-tree
section and keeps baseline statuses unchanged until the work lands.

## Baseline and rules

- Baseline is `5de2f5b`. Uncommitted work in the working tree is not part of this plan
  and must not be counted as progress. Finish or discard it explicitly before starting
  M1.
- Inventory IDs are stable. A milestone lists the IDs it closes. A milestone is done
  only when every listed ID reaches `VERIFIED`, or is explicitly moved out with a
  reason.
- A feature is not done because it renders once. It needs a committed test or a
  recorded runtime check, and export coverage where the feature affects output.
- No milestone may add draw.io code, shaders, icons, fonts, templates, or shape assets.
  Behavior is reimplemented from documentation and observation. Vendor icon packs stay
  blocked until a license decision exists.
- No destructive global restyle. A command that changes many elements must say what it
  will change, operate on an explicit scope, and produce one undo step.
- Page settings are document data, not editor preferences. They travel with the page and
  survive reload. App preferences stay in `options.json`.
- Unsupported features must say so in the UI, in the import result, or in both. Silent
  loss is a bug.

## Milestones

| Milestone | Scope | State | Depends on | Inventory IDs |
|---|---|---|---|---|
| M0 | Baseline inventory and decisions | Done | none | all |
| M1 | Format sidebar integration and page settings | Current | M0 | UIX-01 to UIX-04, DOC-08 to DOC-13, CAN-04 to CAN-06 |
| M2 | Connector labels, waypoints, and routing polish | Next | M1 | CON-09 to CON-12, CON-15, CON-18, CON-21 |
| M3 | Layers | Planned | M1, M2 | ORG-13 to ORG-15, DIA-12 |
| M4 | Shape registry, search, and libraries | Planned | M1, M3 | CUS-05, CUS-07 to CUS-16, SHP-17 |
| M5 | draw.io import and export adapter | Planned | M2, M3, M4 | IO-15 to IO-18, IO-21, DOC-15 |
| M6 | Diagram data, links, tags, and find | Planned | M5 | DIA-01 to DIA-09 |
| M7 | Templates and diagram packs | Planned | M4, M6 | GEN-06 to GEN-15, CUS-12 to CUS-14 |
| M8 | Data and text generation | Blocked in part | M5, M7 | GEN-01 to GEN-05 |
| M9 | Collaboration and integrations | Blocked | none scheduled | STO-02 to STO-11 |

## M0: Baseline

**Purpose.** Record what exists, what is absent, and what evidence supports each claim.

**Outputs.**

- `feature-inventory.md` with 231 capabilities and per-domain counts.
- `architecture-decisions.md` with the decisions that constrain later work.
- `verification-log.md` with the commands, dates, and limits of this session.
- A source corpus check against 220 draw.io documentation pages under `.artifacts/`.

**Exit criteria.** Every row has a stable ID, a source URL, a module or `None`, a status,
acceptance criteria, and a limitation. Counts match the tables.

## M1: Format sidebar integration and page settings

**State: current.** The inspector at HEAD already has Style, Text, and Arrange tabs plus
a Canvas section for grid size and snap. That is a useful base, not the target. The target
is draw.io's behavior: the panel changes with the selection, an empty selection exposes
diagram settings, and controls live in sections that can be understood without reading
the source.

**Why first.** The format panel is the control surface for the rest of the roadmap.
Connector label controls, layer membership, shape properties, and import options all need
a place to live. Page settings also unblock the grid, background, paper, and page-view
rows that currently have no home.

**Working tree (2026-11-16).** Superscript and subscript also landed (TXT-20): the Text
tab gains x² and x₂ toggles after S, and a text style or span carries a script value of
normal, super, or sub. With no text selection the toggle sets the whole node's base;
inside a text selection it overrides only the selected characters. Applying one script
replaces the other, and pressing the active toggle returns the range to normal.
Superscript and subscript glyphs draw at 0.7 of the base size and shift the baseline by
-0.35 em and 0.15 em, while line boxes keep the base size, so only the glyph size,
advance, and baseline move. Rich and bidirectional layout share the same metrics, clear
formatting resets the value, and the page file round-trips both the base and per-span
script.

**Working tree (2026-11-17).** Rotation also landed (SHP-19): a node stores a clockwise
angle and a Keep label upright flag. A circular handle above the selection's top-right
corner turns an unlocked single shape or empty frame by 15 degrees, or 1 degree while
Shift is held. The Arrange tab adds an exact Angle field and a Rotate shape only by 90
degrees button. Dragging and the quarter-turn button turn the label with the shape
unless the flag is set, and the exact field follows the flag, matching draw.io's
Arrange tab. Rotated shapes hit test through their transform, the overlay draws the
handle, and the angle and flag join undo and redo and survive save, reload, and PNG,
SVG, and draw.io export. Groups, frames with children, and connectors have no handle,
and draw.io export has no upright-label style.

**Working tree (2026-11-18).** UIX-04 also landed on
`agent/drawio-features`: format sections collapse from their headers, and the
open or closed state is saved in app options so it survives a restart. A filter
field above the sections keeps matching groups open, hides nonmatching rows, and
restores the saved collapse state when cleared. A query with no matches returns
a clear message. Six inspector tests and the options round-trip test cover the
behavior; runtime proof is in `.artifacts/round29/`.

**Working tree (2026-11-18, round 30).** CON-17 also landed: each connector end
can be fixed at a normalized point or floating on the bound shape's outline.
The mode is stored as a defaulted binding flag, so earlier documents keep fixed
ends. A floating end follows the target rect center, and switching back to
fixed stores the current point. draw.io import treats a missing exit or entry
coordinate as floating, while export writes those keys only for fixed ends. The
Arrow section edits Start and End independently in one undo step. Four tests
cover the panel, geometry, import and export, and undo/redo. Live evidence with
five passing assertions is in `.artifacts/round30/`.

**Working tree (2026-11-18, round 31).** CON-13 also landed: connector heads
carry separate start and end sizes and offsets. Sizes scale the native head
geometry around draw.io's default marker size 6, and positive perimeter spacing
moves each tip into the line. draw.io import and export map startSize, endSize,
sourcePerimeterSpacing, and targetPerimeterSpacing, writing only non-default
values. The Arrow section edits all four fields through the coalesced undo path.
Seven tests cover geometry, native JSON, draw.io round trips, panel text, and
history. Live evidence with six passing assertions is in `.artifacts/round31/`.

**Working tree (2026-11-18, round 32).** STY-22 also landed on
`agent/drawio-features`. Pages store an adaptive mode of auto, simple, or none.
The first version computed dark variants with Auto. Round 34 corrected that:
Auto keeps authored colors and explicit dark pairs, Simple flips only neutral
black and white, and None draws the page light. Light values stay saved as
authored; fills, strokes, gradient stops, text, labels, and page backgrounds can
carry optional dark values. Colors resolve at the document boundary, the renderer
keys its cache on the resolved appearance, draw.io maps `light-dark()` and
`adaptiveColors`, PNG and SVG exports take `--appearance`, and the Diagram tab
edits the mode and dark colors. Dark-theme text now renders in every mode.
Evidence is in `.artifacts/round32/` and `.artifacts/round34/`.

**Working tree (2026-11-18, round 33).** STY-21 also landed on
`agent/drawio-features`: ten named schemes are available in the global and
selected-element Style tabs. Page scope restyles eligible nodes and sets the page
scheme, background, and grid, while selection scope restyles only selected
subtrees. The command previews and reports changed, locked, hidden, out-of-scope,
and unstyleable nodes, skips locked, hidden, and group nodes, and preserves rich
text, metadata, component overrides, and adaptive color pairs. One undo step
covers the whole edit. draw.io export writes sketchScheme on mxGraphModel and
import validates the name. The first failure showed locked nodes being restyled
and dark values dropped; explicit horizontal card rows also fixed an egui Grid
overlap. Focused tests, a verified live video, state checks, and export proof
are in `.artifacts/round33/`.

**Working tree (2026-11-18, round 34).** STY-13 and DOC-13 also landed:
shape shadow and the page-wide Shadow setting. A page flag and an optional
per-element flag stay separate, with the element value winning. Draw ops put a
translucent copy of the fill and stroke at (3, 4); SVG bounds and scene culling
include it, while hit testing does not. draw.io import and export map shadow=1
and shadow=0, and the Diagram and Style panel edits join undo. The round also
corrected the dark-theme color regression: Auto keeps authored fills and
explicit dark pairs, scheme restyling preserves saved dark values, and the
renderer lifts only low-contrast glyphs against their actual background.
Focused tests, live Xvfb evidence, and export pairs are in `.artifacts/round34/`.

**Working tree (2026-11-18, round 35).** STY-14 also landed on
`agent/drawio-features`. A node stores `style.glass` as a defaulted bool that
native JSON omits when false. The renderer draws a three-stop white gradient
over the fill after the shadow and below the label, using the shape path so
rounded corners follow the outline. The Style tab Effects section edits the
flag through undo and redo. draw.io import reads `glass=1` and `glass=0`, while
export writes only `glass=1` because false is the default. The first draw.io
test failed before the mapping. Five focused tests pass, and the final skip run
passes 473 tests. Live Xvfb `:103` evidence has a verified 89.87 s video with
four passing assertions, pixel samples, PNG exports and a CLI draw.io round
trip in `.artifacts/round35/`.

**Committed (2026-11-19, round 36).** STY-23 closed on
`agent/drawio-features`. The Style tab now carries a static typed property
registry with draw.io-like keys. Its local search filters keys, values edit the
real style and join undo, invalid values show an error without changing state,
and connectors show connector keys instead of shape keys. The same round fixed
the user report that dark-canvas arrows and text read as gray: the renderer
targets 14:1 contrast on dark pages, lifting GPU arrow pixels from 133 to 235
while light exports keep authored colors. The first panel test failed with no
Property / Value section, and the first contrast test failed with authored ink
at 0.52. Focused tests, final checks and before and after frames are in
`.artifacts/round36/` and `.artifacts/contrast/`. The implementation was pushed
to `ibrahemesam/sketchforge` as 7261dfd; this documentation closeout completes
the round.

**Committed (2026-11-19, round 37).** UIX-14 closed on
`agent/drawio-features`. A fresh store opens a three-step welcome panel that
walks through shapes, connectors and export. Dismissal persists in options, and
the help dialog can reopen the tour. The first tests did not compile because
there was no onboarding flag or welcome state. Focused tests, final checks and
a verified live recording are in `.artifacts/round37/`. The first live run is
marked discarded because it clicked Skip; the corrected run is in
`.artifacts/round37/live2/`. The implementation was pushed to
`ibrahemesam/sketchforge` as 858d1a0; this documentation closeout completes
the round.

**Committed (2026-11-19, round 38).** TXT-21 closed on
`agent/drawio-features`. `TextStyle.paragraphs` carries a list kind and indent
level for each hard-break paragraph. Marker glyphs and numbering render with
indent, list buttons apply to all paragraphs in one undo step, and Indent and
Outdent affect the active paragraph. The saved page round-trips the values while
ordinary text stays unchanged. The first failure had no list metadata or Text
tab controls. Focused tests, final checks and a verified live recording are in
`.artifacts/round38/`. The parallel skip run hit an intermittent SIGSEGV in the
mcp test binary, so the final run used one test thread and passed 486 tests.
The implementation was committed as 5802ded; this documentation closeout
completes the round.

**Committed (2026-11-19, round 39).** IO-19 closed on
`agent/drawio-features`. Ctrl/Cmd+P opens a Print dialog with paper, orientation,
fit or custom scale, margin, and page or selection scope. `io::pdf` emits a
paper-sized MediaBox and centers the scaled content inside the margins. The
primary button writes the PDF to the project folder without a native picker so
the result is deterministic; Choose location keeps the rfd save dialog. The
first failure had no print API. Focused, integration and final checks pass, and
a verified 490.87 s live recording is in
`.artifacts/round39/evidence/evidence-20261119-124018-fd7149/report.md` with the
printed PDF beside it. The implementation was committed as 97dfc51; this
documentation closeout completes the round.

**Committed (2026-11-19, round 40).** DOC-09 closed on
`agent/drawio-features`. A page stores an image asset id, fit mode and opacity.
The asset registry owns the bytes by content hash, and save, embedded bundles and
reload carry them. The renderer paints the image in screen space behind the grid
and content, and PNG, SVG and PDF exports include it for whole-page scopes. The
Background dialog gains Choose image, Clear, fit mode and opacity controls. The
first failure had no background image model. Focused tests, the serial skip suite
(503 tests), final checks and a verified 37.10 s live recording in
`.artifacts/round40/evidence/evidence-20261119-132330-5a28e0/report.md` are in
`.artifacts/round40/`. Using another page as a background stays deferred because
it needs cycle checks. The implementation was committed as a63e090; this
documentation closeout completes the round.

**Committed (2026-11-19, round 41).** TXT-24 closed on
`agent/drawio-features`. `TextStyle.curve` stores a fixed arc height, and layout
lifts glyph baselines along the arc while keeping the glyphs upright. The Text
tab gains an ARC toggle, draw.io style maps `textPath=1;` in both directions, and
MCP `update_styles` accepts `textPath`. The first failure had no `curve` field.
Four focused tests, the serial skip suite (507 tests), final checks and a
verified 38.60 s live recording in
`.artifacts/round41/evidence/evidence-20261119-134656-0f3f64/report.md` are in
`.artifacts/round41/`. The live run toggled the path through MCP rather than the
ARC button because bare Xvfb does not place that control reliably. draw.io's
isometric-face mode stays deferred. The implementation was committed as fc0f065;
this documentation closeout completes the round.

**Committed (2026-11-19, round 42).** CAN-06 closed on
`agent/drawio-features`. The page setting, renderer and Diagram tab control for
a custom grid color already existed. Draw.io has no standard page grid-color
attribute, so export now writes a `__sketchGridColor` sidecar on `mxGraphModel`
and import restores it; the tooltip no longer claims the value is not exported.
The first failure showed the sidecar missing from export. Three focused tests,
the serial skip suite (510 tests), final checks and a verified 42.77 s live
recording in
`.artifacts/round42/evidence/evidence-20261119-135940-3eab09/report.md` are in
`.artifacts/round42/`. The live run seeded page JSON because the native color
picker is unreliable under bare Xvfb. The implementation was committed as
727c534; this documentation closeout completes the round.

**Committed (2026-11-19, round 43).** DIA-10 closed on
`agent/drawio-features`. A page now stores a fade transition and a duration from
100 to 3000 ms, edited on the Diagram tab. Switching pages arms the fade on the
next tick so an idle editor clock cannot finish the first frame at once; a
translucent veil fades out over the new page. Draw.io has no page-transition
attribute, so export writes `__sketchPageTransition` and
`__sketchPageTransitionMs` sidecars that draw.io ignores and Sketch restores on
import. The first failure had neither the model nor the `page_transition` field;
the first live run then exposed the stale-clock bug. Five focused tests, one
inspector test, the serial skip suite (516 tests), final checks and a verified
31.60 s live recording in
`.artifacts/round43/evidence/evidence-20261119-142023-9dad02/report.md` are in
`.artifacts/round43/`. The live run seeded the two-page project in JSON, and the
Diagram tab checkbox is covered by a unit test rather than a live click. The
implementation was committed as 9cae03e and cb8a4fd; this documentation closeout
completes the round. DIA-11 animation actions stay blocked on a sandboxed action
interpreter.

**Committed (2026-11-19, round 44).** CUS-05 closed on
`agent/drawio-features`. A new `src/io/library_xml.rs` adapter reads the JSON
array inside `<mxlibrary>`, passes each entry's uncompressed mxGraphModel
through the existing draw.io importer and writes models back through the draw.io
exporter. Imported shapes become personal library items and insert with the
current Library tab path. The tab gains Import XML and Export XML buttons,
backed by tested file helpers, and the CLI gains `--import-lib` and
`--export-lib`. The first failure had no adapter module; the first focused run
then found that draw.io shorthand styles such as `rhombus;` imported as
rectangles, which the importer now handles. Six focused tests, the serial skip
suite (522 tests), final checks and a verified 38.53 s live recording in
`.artifacts/round44/evidence/evidence-20261119-144811-d04937/report.md` are in
`.artifacts/round44/`. The live run showed the imported tile, inserted it on the
canvas and checked the CLI export; the import used the CLI because bare Xvfb
cannot drive the native file picker. Compressed or encrypted library entries
are reported as unsupported. The implementation was committed as 60186ab; this
documentation closeout completes the round.

**Committed (2026-11-19, round 45).** TXT-06 closed on
`agent/drawio-features`. The inspector picker already read discovered system
families, but the rest of the app still claimed Inter was the only face.
`text::family_info()` now reports real availability and the faces found by the
background scan, with Inter bundled in four styles. MCP get_font_family_info
uses it, get_basic_info lists `font_families()` instead of a hard-coded array,
and `fontFamily` writes `text_style.family` in CSS and maps on shape labels in
draw.io import as well as export. The first failure showed Adwaita Mono
unavailable with no styles. Five focused integration tests and one unit test,
the serial skip suite (528 tests), final checks and a verified 83.03 s live
recording in
`.artifacts/round45/evidence/evidence-20261119-150844-eb3842/report.md` are in
`.artifacts/round45/`. The live run used MCP over HTTP and polled until the
background font scan landed; it did not click the inspector picker. The code
was committed after the recording, so the report names the previous commit. The
implementation was committed as c7941e0; this documentation closeout completes
the round.

**Committed (2026-11-19, round 46).** CON-18 closed on
`agent/drawio-features`. The row was stale: `Editor::reverse_connectors`, the
Arrange tab Reverse button and two editor tests were already at HEAD. The real
gap was MCP, so `reverse_connectors` now takes `nodeIds`, resolves each on its
own page, reverses connector points and swaps the start and end bindings in one
undo step per page, and reports `reversed`, `count`, `notConnectors` and
`notFound`. Heads stay on the line's own ends and move to the opposite shape,
matching draw.io's `turnShapes` edge branch. The first failure was
`unknown tool 'reverse_connectors'`. A first focused run asserted hard-coded
points and failed because insertion reroutes the arrow; the tests now compare
with the points captured before the call. Two integration tests, the serial
skip suite (530 tests), final checks and a verified 33.73 s live recording in
`.artifacts/round46/evidence/evidence-20261119-152715-46ade4/report.md` are in
`.artifacts/round46/`. The live flow called MCP over HTTP rather than clicking
the inspector button, read bindings from the saved page JSON because
`get_node_info` omits connector ends, and measured canvas pixels only so status
text changes would not mask the undo result. The code was committed after the
recording, so the report names `unknown`. The implementation was committed as
221cf0f; this documentation closeout completes the round.

**Committed (2026-11-19, round 47).** DIA-05 closed on
`agent/drawio-features`. The row was stale: `Node.tags`, the Data tab editor,
the find bar's tag search and draw.io's tags attribute were already at HEAD.
The MCP `find_nodes` tool had no tag query, so a tag-only search was rejected
and a combined query ignored the tag. It now accepts `tag` with the same
case-insensitive `*` glob used for text, reports the stored tag in `matched`,
combines with text and style filters using AND, and honours page and subtree
scope. Four integration tests, the serial skip suite (534 tests), final checks
and a verified 30.27 s live recording in
`.artifacts/round47/evidence/evidence-20261119-154205-45489a/report.md` are in
`.artifacts/round47/`. The first focused run expected a lowercase match for a
node tagged `Web`; the tool correctly reports the stored value. The live flow
called MCP over HTTP rather than clicking the editor find bar, seeded tags in
the page JSON, and confirmed `get_node_info` reports them. The code was
committed after the recording, so the report names `unknown`. The
implementation was committed as 2ec2fbd; this documentation closeout completes
the round.

**Committed (2026-11-19, round 48).** DIA-04 closed on
`agent/drawio-features`. The data side was already at HEAD: `Node.tooltip`, the
Data tab editor, find and replace, draw.io's tooltip attribute and
`get_node_info` reporting. The unmet part was the acceptance criterion that
the tooltip "truncates cleanly". The first failure showed a 600-character
tooltip at full length instead of 201 characters. `tooltip_display_text` now
clips resolved text to 200 characters on a character boundary with a trailing
ellipsis, wraps at 260 px and constrains the popup to the canvas. Two unit
tests, the serial skip suite (536 tests), final checks and a verified 20.00 s
live recording in
`.artifacts/round48/evidence/evidence-20261119-160259-ac7a91/report.md` are in
`.artifacts/round48/`. Unlike recent MCP-only rounds, this flow hovered the
shape with real `xdotool` pointer moves on Xvfb `:105` and measured a bounded
261x84 px popup that clears on leave. A 1024-character tooltip was seeded in
the page JSON; the ellipsis is checked by the unit test rather than OCR. Two
earlier recordings were stopped and replaced. The code was committed after the
recording, so the report names `unknown`. The implementation was committed as
cca0145; this documentation closeout completes the round.

**Committed (2026-11-19, round 49).** DIA-03 closed on
`agent/drawio-features`. The row was stale: node links, Cmd/Ctrl click, the Data
tab Open Link button, page targets, the safe scheme list, draw.io mapping and
`get_node_info` reporting were already at HEAD. MCP could read links but had no
way to edit them. The new `set_link` tool takes a batch of node ids and targets,
clears a link on an empty string, rejects unsafe targets with the editor's
scheme check, and records each call as one undo step. Four integration tests,
the serial skip suite (541 tests), final checks and a verified 37.53 s live
recording in
`.artifacts/round49/evidence/evidence-20261119-174935-45d106/report.md` are in
`.artifacts/round49/`. The flow used MCP over HTTP rather than the Data tab. It
stored a page target but did not open it in a viewer, and the first recording
was discarded because its last frame went black. The report names baseline
`fce3746` because the code was committed after the recording. The
implementation was committed as ea87c41; this documentation closeout completes
the round.

**Committed (2026-11-19, round 50).** SHP-19 closed on
`agent/drawio-features`. Native rotation and the Keep label upright flag were
already at HEAD, but draw.io export dropped `label_upright` and import never
read it back. `export_and_reimport_keep_an_upright_label_flag` failed first
because the exported mxfile had no sidecar for the flag. draw.io export now
writes a native `__sketchLabelUpright` value in `sketch-data`, and import
consumes it without leaving metadata on the node. The draw.io test suite passes
(11 tests), and the serial skip suite passes 542 tests with the known
`m2_pages_panel_screenshot` failure skipped. A CLI export of a seeded rotated
upright-labeled shape produced `.artifacts/round50/io/upright.drawio` with
`rotation=30` and the sidecar, and reimporting it produced
`.artifacts/round50/io/imported-node.json` with rotation 30 and
`label_upright` true. The implementation was committed as 30636ea; this
documentation closeout completes the round.

**Round 51 (committed 4e5b64d, pushed).** Closed stale parity rows DIA-02, DIA-06, CUS-16, DIA-01, SHP-17, UIX-04 and UIX-11 as evidence-only doc updates against HEAD behavior and existing tests. DIA-07 was implemented and verified with focused checks: replace-one/current-match support in `src/find.rs` and `src/ui/findbar.rs`, covered by `replace_one_changes_only_the_current_match_and_is_one_undo_step` and `cargo test --lib find`. Parity docs recorded VERIFIED 135 and IN_PROGRESS 3 at that commit.

**Round 52 (committed 5ffd939, pushed).** Closed stale DISCOVERED rows CON-09, CON-11, CON-15, TXT-20, GEN-06, CUS-07, CUS-08, CUS-09 and CUS-12 as evidence-only doc updates against HEAD behavior and existing tests. No application code changed.

**Round 53 (committed a47c473, pushed).** Closed stale ANALYZED/DISCOVERED rows CON-10, CON-12, CON-16, CON-19, DOC-08, DOC-10 and DOC-11 as evidence-only doc updates against HEAD behavior and existing tests. No application code changed.

**Round 54 (committed 133f255, pushed).** Closed stale BLOCKED/DISCOVERED rows DIA-08, DIA-09 and DOC-14 as evidence-only doc updates against HEAD behavior and existing tests. No application code changed.

**Round 55 (committed 5e08e52, pushed).** Closed stale DISCOVERED rows CAN-15 and CAN-16 as evidence-only doc updates against HEAD behavior and existing tests. No application code changed.

**Round 56 (committed 3436e65, pushed).** Closed stale ANALYZED/BLOCKED rows IO-15, IO-16 and IO-21 and refreshed CON-17 evidence as evidence-only doc updates against HEAD behavior and existing tests. No application code changed.

**Round 57 (committed 15863fe, pushed).** Closed stale IMPLEMENTED/DISCOVERED rows CON-13 and GEN-01/02/03/05 as evidence-only doc updates against HEAD behavior and existing tests. No application code changed.

**Round 58 (committed 6080801, pushed).** Closed stale BLOCKED rows CUS-13 and CUS-14 as evidence-only doc updates against HEAD behavior and existing tests. No application code changed.

**Round 59 (committed b2380a6, pushed).** Closed stale DISCOVERED row TXT-19 as a code+doc verification update: added vertical_text_stacks_characters_downward in `src/text.rs` and updated parity docs against HEAD vertical text behavior.

**Round 60 (committed 8f85118, pushed).** Closed stale DISCOVERED rows GEN-07, GEN-08, GEN-09, GEN-11, GEN-12, GEN-13, GEN-14 and GEN-15 as evidence-only doc updates against HEAD bundled template packs and existing tests, with limitations preserved where acceptance remains partial. No application code changed.

**Round 61 (committed 8734350, pushed).** Cleaned stale "Working tree" wording on blocked rows DOC-15, GEN-04 and GEN-10 to reflect committed HEAD state while keeping statuses/acceptance limitations unchanged. No application code changed.

**Round 62 (committed 7a81b40, pushed).** Closed stale DISCOVERED row CUS-11 as an evidence-only doc update against HEAD shape metadata/property editing and existing tests. No application code changed. Parity docs recorded VERIFIED 167, IMPLEMENTED 30, DISCOVERED 1, BLOCKED 29, ANALYZED 1 and IN_PROGRESS 3.

**Round 63 (committed d2fe51c, pushed).** Closed stale ANALYZED/BLOCKED rows ORG-13, ORG-14 and ORG-15 as evidence-only doc updates against HEAD page-layer behavior and existing tests. No application code changed. Parity docs recorded VERIFIED 170, IMPLEMENTED 30, IN_PROGRESS 3, ANALYZED 0, DISCOVERED 1 and BLOCKED 27.

**Round 64 (committed f43aca4, pushed).** Closed stale IN_PROGRESS rows DOC-12 and UIX-03 as evidence-only doc updates against HEAD per-page grid settings and Diagram-tab behavior (DOC-12 IMPLEMENTED, UIX-03 VERIFIED) and refreshed UIX-02 limitations while keeping it IN_PROGRESS. No application code changed. Parity docs recorded VERIFIED 171, IMPLEMENTED 31, IN_PROGRESS 1, ANALYZED 0, DISCOVERED 1 and BLOCKED 27.

**Round 65 (committed dedcd0a, pushed).** Added integration test two_pages_keep_different_grid_settings_across_switch_save_and_reload in tests/projects.rs proving two pages keep different grid visibility/size/color across page switches, save, and reload. DOC-12 moved from IMPLEMENTED to VERIFIED. No runtime behavior changed. Parity docs recorded VERIFIED 172, IMPLEMENTED 30, IN_PROGRESS 1, ANALYZED 0, DISCOVERED 1 and BLOCKED 27.

**Round 66 (committed cb6a98c, pushed).** Closed stale IMPLEMENTED row UIX-01 as an evidence-only doc update against HEAD tabbed-inspector behavior and existing tests (UIX-01 VERIFIED). No application code changed. Parity docs recorded VERIFIED 173, IMPLEMENTED 29, IN_PROGRESS 1, ANALYZED 0, DISCOVERED 1 and BLOCKED 27.

**Round 67 (committed a187293, pushed).** Advanced UIX-02 with selection-aware element tab computation: the Text tab is omitted when no selected element carries editable text or a label, and the active tab is clamped to a valid tab for the current selection context. Added/updated ui tests in src/ui/mod.rs (inspector_tabs_follow_selection_presence, inspector_element_tabs_depend_on_text_capability). UIX-02 remains IN_PROGRESS because connector and standalone text-layer selections still receive the same tab set and empty/irrelevant sections within tabs are not fully eliminated.

**Round 68 (committed d23302f, pushed).** Advanced UIX-02 by making Style-tab connector/sticky/sketch sections resolve from any selected element, not only the first selected node, so mixed selections that include a connector still show the Arrow section. Added inspector tests covering connector-only style sections (Arrow shown, Fill hidden), text-only style sections (Arrow and Fill hidden), and mixed rect+connector selection (Arrow shown). Connector and standalone text-layer selections still receive the same top-level tab set because both have text targets; some irrelevant controls/sections may remain in non-Style tabs, so UIX-02 remains IN_PROGRESS.

**Round 69 (committed a17d5a5, pushed).** Advanced UIX-02: unlabeled connector-only selections now omit the Text tab (Style, Arrange, Data) while text layers keep Style, Text, Arrange, Data, giving different tab sets for the typical connector vs text-layer case. Unlabeled single connectors show an Add label button in the Style tab Arrow section; labeled connectors still include Text. Added/updated tests in src/ui/mod.rs and src/ui/inspector.rs. UIX-02 remains IN_PROGRESS because a labeled connector shares the same top-level tab set as a text layer and some irrelevant controls/sections may remain in non-Style tabs.

**Round 70 (committed 848ac44, pushed).** Verified IO-06 Transparent PNG background. Added transparent_png_export_omits_page_background in tests/io.rs, exporting a frame-containing page with transparent: true and asserting corner pixels have alpha 0 while content remains visible; strengthened embedded_svg_and_png_reopen_all_pages with a transparent-pixel assertion. IO-06 moved from IMPLEMENTED to VERIFIED.

**Round 71 (committed e3d9878, pushed).** Verified SHP-10 Flip horizontally and vertically. Added shp10_flip_horizontal_mirrors_geometry_and_keeps_box_and_gradient in src/editor/commands.rs tests, asserting an asymmetric path with a linear gradient keeps its rect/fills and mirrors geometry on horizontal flip. SHP-10 moved from IMPLEMENTED to VERIFIED.

**Round 72 (committed f3d8f4e, pushed).** Verified STY-19 Copy and paste style. Added sty19_copy_paste_style_transfers_properties_and_clears_text_spans in src/editor/commands.rs tests, asserting style properties transfer to two targets and text style transfers while rich-text spans are cleared on text layers. STY-19 moved from IMPLEMENTED to VERIFIED.

**Round 73 (committed a6d27e1, pushed).** Verified STY-10 Per-corner radius. Added sty10_per_corner_radius_changes_only_selected_corner in src/model/node.rs tests, asserting per-corner radii change only the selected corner and survive JSON roundtrip. STY-10 moved from IMPLEMENTED to VERIFIED.

**Round 74 (committed df363cb, pushed).** Verified TXT-15 Text background color and TXT-17 Text shadow. Added txt15_text_background_color_roundtrips and txt17_text_shadow_flag_roundtrips in src/model/node.rs tests, confirming text background colors and text shadow flag persist through JSON roundtrip. TXT-15 and TXT-17 moved from IMPLEMENTED to VERIFIED.

**Round 75 (committed 8088e64, pushed).** Verified IO-07 Export scale. Added io07_png_export_scale_changes_pixel_dimensions in tests/io.rs, asserting 1x/2x/4x PNG export pixel dimensions scale exactly. IO-07 moved from IMPLEMENTED to VERIFIED.

**Round 76 (committed 878c940, pushed).** Verified STY-08 Line caps and joins. Added sty08_line_caps_and_joins_roundtrip in src/model/node.rs tests, confirming all Cap/Join values roundtrip through JSON on node style. STY-08 moved from IMPLEMENTED to VERIFIED.

**Round 77 (committed d94609e, pushed).** Evidence-only verification of CAN-10 Equal-spacing guides. spacing_equalizes_gaps in src/editor/snap.rs tests already proves snap_spacing equalizes gaps and emits gap guides. CAN-10 moved from IMPLEMENTED to VERIFIED.

**Round 78 (committed ddff74b, pushed).** Verified ORG-10 Auto layout wrapping. Added org10_wrap_flows_children_into_second_row in src/layout.rs tests, confirming a wrap-enabled narrow frame flows the third child onto a second row. ORG-10 moved from IMPLEMENTED to VERIFIED.

**Round 79 (committed 871801f, pushed).** Verified IO-13 Image import and drag-and-drop. Added io13_image_import_roundtrips_through_project_reload in tests/projects.rs, asserting inserted image fill, non-zero size, and embedded asset survive project save/reload. IO-13 moved from IMPLEMENTED to VERIFIED.

**Round 80 (committed e1cfd10, pushed).** Verified TXT-16 Text border and SHP-26 Place images. Added txt16_text_border_roundtrips in src/model/node.rs tests and shp26_placed_image_move_resize_survives_reload in tests/projects.rs. TXT-16 and SHP-26 moved from IMPLEMENTED to VERIFIED.

**Round 81 (committed a4e5a4f, pushed).** Added simple-layout Justify support for TXT-09. Added TextAlign::Justify, inspector Justify control, simple-layout word-gap justification, and layout_justify_distributes_word_gaps in src/text.rs tests. TXT-09 moved from IMPLEMENTED to VERIFIED with honest limitations (rich/bidi layout falls back to left for Justify; draw.io export maps Justify to left).

**Round 82 (committed 9d6cac1, pushed).** Verified CUS-11 Shape properties dialog using existing tests import_reads_user_object_metadata_links_tooltips_and_tags and node_info_reports_links_tooltips_tags_and_metadata. CUS-11 moved from IMPLEMENTED to VERIFIED with limitation: no separate generic typed-property schema beyond metadata map and existing typed fields. Targeted tests and `python3 .artifacts/check_parity_docs.py` passed before commit.

**Round 83 (committed 90b1680, pushed).** Verified CON-21 Auto-route around obstacles. Added con21_elbow_route_avoids_third_shape in src/connector.rs tests; router avoids visible bindable third shape in routing area. CON-21 moved from IMPLEMENTED to VERIFIED. Targeted test, fmt, clippy -D warnings, diff --check, and parity docs check passed before commit.

**UIX-02 increment (committed f8e4a03, pushed).** Captured failing test connector_only_arrange_tab_hides_shape_only_controls, then hid Autosize/Copy Size/Apply Size/Constrain Proportions for connector-only Arrange selections. UIX-02 remains IN_PROGRESS: labeled connector still shares top-level tab set with text layer and other irrelevant controls may remain.

**SHP-11/12/13 verification (committed f5e091a, pushed).** Added shape_arrange_tab_exposes_position_size_and_size_tools in src/ui/inspector.rs tests confirming Arrange tab exposes X/Y/W/H, Constrain Proportions, and Copy Size controls for shape selections. SHP-11/12/13 moved from IMPLEMENTED to VERIFIED.

**STY-17/18 verification (committed 3fbd2e8, pushed).** Added style_tab_exposes_palette_section in src/ui/inspector.rs tests; cited existing selected_style_tab_exposes_scheme_scope and editor scheme apply/preview tests. STY-17/18 moved from IMPLEMENTED to VERIFIED with honest notes on actual palette/scheme counts.

**UIX-02 increment (committed 3f1f409, pushed).** Extended connector-only Arrange hiding to Angle, Rotate by 90°, Keep label upright, Connector default, and Flip after capturing failing test. UIX-02 remains IN_PROGRESS.

**UIX-02 increment (committed 3b921d6, pushed).** Connector-only Style tab now hides Rounded, Glass, Corner, and Rotation after capturing failing test. UIX-02 remains IN_PROGRESS.

**UIX-02 increment (committed 61e783f, pushed).** Connector-only selections omit Data tab and render Data sections in Arrange so metadata stays accessible; labeled connector tab set now differs from text layer. UIX-02 remains IN_PROGRESS pending further irrelevant-control audit.
**UIX-02 increment (committed a3b73a2, pushed).** Captured failing tests, then aligned connector Arrange with the draw.io docs: kept Flip and Reverse, hid Size/Position fields, Align and Snap to Grid. Added a shape-regression test so those controls stay available for shapes. UIX-02 remains IN_PROGRESS pending review of Selection/Layout sections.
**DIA-12 increment (committed b4607aa and e98e2d9, pushed).** Added a safe `data:action/json,` interpreter for toggle, show, hide and select over layers, cells and tags. Unsupported effects are refused, draw.io ids are remapped on import and export, and MCP accepts these links. Click, runtime, parser, round-trip and MCP tests cover the path. DIA-12 is VERIFIED; DIA-11 remains BLOCKED for the full action set.
**UIX-02 increment (committed f478440, pushed).** Connector-only Arrange no longer renders an empty Selection section or a Flatten button that would turn the connector into a path. Shape selections keep Selection and Flatten. UIX-02 remains IN_PROGRESS pending the Layout section review.

**Round 86 (code committed 54a4358).** Added right-drag canvas panning on the existing pan path and replaced the plus-circle quick-connect knobs with outward blue arrowheads. Selected shapes now show the eight fixed-point crosses as well as the arrows, and the help footer names right-drag. Failing mesh and input tests were captured first; CAN-01, CAN-14 and CAN-15 keep VERIFIED status with updated evidence.

**Round 87 (code committed f2ad0df).** A right click without movement now selects the shape under the pointer and opens a canvas context menu; a right drag still only pans. The popup uses the editor action table for edit, clipboard, duplicate, delete, arrange, group and ungroup commands, and a smaller set on empty canvas. Escape, an outside click or a chosen command closes it. Failing input tests were captured before the fix, and a headless render confirms the popup paints over the diagram. CAN-01 keeps VERIFIED status with updated evidence.

**Round 88 (code committed 8d23cea).** Connector-only Arrange no longer shows Layout or Constraints when a connector sits inside a frame. A shape in the same frame keeps both, with a render test on each side of that split. UIX-02 moves to VERIFIED after the tab, section and control audit.

**Round 89 (code committed 83c228c).** Escape now closes the canvas context menu without clearing the current selection. The editor tracks popup state and only falls through to the normal Escape behavior when no menu is open. Outside clicks and chosen actions clear the same state. CAN-01 keeps VERIFIED status with updated evidence.

**Round 90 (code committed 14e9272).** Icon buttons now register AccessKit button labels from the generated tree update, with specific names for toolbar and top-bar actions. The label test failed before the fix, while the Tab-focus test passed both times and now guards the behavior. UIX-12 moves from DISCOVERED to IN_PROGRESS because there is still no full command audit, visible-focus assertion, or canvas accessibility tree.

**Round 91 (code committed 2e0e1df).** The Layers panel order buttons now expose Send backward and Bring forward to AccessKit instead of sharing the generic Collapse name. A sidebar render test failed on the two duplicate labels before the fix. UIX-12 stays IN_PROGRESS while the broader audit, visible focus, and canvas tree remain open.

**Round 92 (code committed b7b3a54).** Sticky note color swatches now expose their six hex colors as AccessKit labels instead of unnamed click targets. A swatch render test failed with no labels before the fix. UIX-12 stays IN_PROGRESS while the command audit, visible focus, and canvas tree remain open.

**Round 93 (code committed 486783d).** Every button in the Layers dialog now has an action-specific AccessKit name instead of a glyph or generic icon name. A dialog render test failed on the missing Hide Layer 2 label before the fix. UIX-12 stays IN_PROGRESS.

**Round 94 (code committed 489e7a6).** The Style palette pager now names both arrows and all three page dots. A render test failed on glyph-only arrow labels before the fix. UIX-12 stays IN_PROGRESS.

**Round 95 (code committed bacbe2f).** The sidebar eye and lock click regions now name their actions. A hidden, locked row produced no labels in the failing test. UIX-12 stays IN_PROGRESS.

**Round 96 (code committed 8af5f2d).** Sidebar page rows now say Switch to <page name> in the accessibility tree. A panel render test found only Add and Collapse labels before the fix. UIX-12 stays IN_PROGRESS.

**Round 97 (code committed d661af8).** Command palette result rows now say Run <command> in the accessibility tree. A palette render test found no labels before the fix. UIX-12 stays IN_PROGRESS.

**Round 98 (code committed 570ad49).** Sidebar layer rows now say Select <layer> in the accessibility tree. A layer render test found no labels before the fix. UIX-12 stays IN_PROGRESS.

**Round 99 (code committed faf817d).** Library shape tiles now say Insert <shape> in the accessibility tree. A library panel render test found no Insert Decision label before the fix. UIX-12 stays IN_PROGRESS.

**Round 100 (code committed 1f17c9f).** Home sidebar rows now say Switch to <view> in the accessibility tree. A home screen render test found no Switch to Recents label before the fix. UIX-12 stays IN_PROGRESS.

**Round 101 (code committed 0931d18).** Layer disclosure chevrons now say Collapse <layer> or Expand <layer> in the accessibility tree. A layer render test saw only the two row labels before the fix. UIX-12 stays IN_PROGRESS.

**User redirect, sidebar (code committed e2663ad).** The left panel keeps egui's resize handle, stores its width in Options, clamps content without fighting a drag, and shows a divider at the edge. Two render tests failed before the fix. IO-15 and IO-16 keep their statuses.

**User redirect, draw.io colors (code committed 21c0b68).** Color import now follows draw.io roles for defaults, invert, CSS syntax, light-dark pairs, swimlane body fill and gradient flags. Export writes gradient=1 and swimlaneFillColor. A real Life.drawio file kept only white fills before the fix, then kept its green and magenta colors. The user also lifted the clean-room restriction for draw.io source. README.md, THIRD_PARTY.md and ADR-006 now record the Apache-2.0 port.

**User redirect, draw.io BPMN shapes (code committed f6b7f69).** BPMN event, task and gateway aliases now import as native registry shapes instead of generic ellipses, rectangles and frames. The `outline` key selects start, intermediate or end events, task cells with `container=1` still become tasks, and native BPMN shapes export with draw.io's `mxgraph.bpmn` styles. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists the BPMN sources alongside the earlier draw.io port.

**User redirect, draw.io C4 shapes (code committed e5faf5c).** Draw.io's `c4Type` property now maps `UserObject` cells to native C4 person, software system, container, component and boundary shapes. `mxgraph.c4` shape aliases cover cells without the property. Native C4 shapes export with the draw.io C4 palette styles and a `c4Type` property, so the two integration tests pass after first failing. THIRD_PARTY.md now lists Sidebar-C4.js.

**User redirect, draw.io SysML shapes (code committed 3311f08).** Draw.io's `mxgraph.sysml.port*` aliases now import as the native SysML port. Block and requirement composite templates are recognised by their `<<block>>` and `<<requirement>>` child cells instead of becoming plain rectangles. Native SysML shapes export with draw.io's port alias and the composite parent style with the same child markers. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists Sidebar-Sysml.js and mxSysML.js.

**User redirect, draw.io flowchart shapes (code committed fb9c602).** Draw.io's `mxgraph.flowchart.document2`, `manual_input`, `manual_operation`, `on-page_reference`, `sequential_data` and `stored_data` aliases now import as native flowchart shapes. Native document, manual input, manual operation, on-page connector, data and database shapes export with the matching draw.io styles. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists Sidebar-Flowchart.js.

**User redirect, draw.io ER notation (code committed 9e98c71).** Draw.io's `double=1` flag on `ext` and `rhombus` cells now imports as Weak Entity and Relationship. Underlined ellipses import as Key Attribute, and dashed ellipses import as Derived Attribute. Native ER shapes export with the same style flags, so all four variants round-trip. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists Sidebar-ER.js.

**User redirect, draw.io UML shapes (code committed 40aa938).** Draw.io's `shape=note2` now imports as Note and `shape=folder` imports as Package. The folder alias wins even when draw.io marks the cell as a container, so it no longer becomes a frame. Native note and package shapes export with the UML palette styles. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists Sidebar-UML25.js.

**User redirect, draw.io floorplan shapes (code committed 03bee77).** Draw.io's `mxgraph.floorplan.*` door, window and stair variants now import as the native Door, Window and Stairs shapes. Native shapes export with the matching stencil styles, so they reimport with their native names. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists Sidebar-Floorplan.js.

**User redirect, draw.io network shapes (code committed 3b80e76).** Draw.io's `mxgraph.networks.*` server, PC and storage aliases now import as the native Server, Host and Database shapes. Native host, server, firewall and cloud shapes export with the matching stencil styles. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists Sidebar-Network.js.

**User redirect, draw.io Person shape (code committed 1ebf465).** Draw.io's mermaid person stencil now imports as the native Person shape, and native Person exports with that stencil and perimeter. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md now lists Sidebar-Basic.js.

**User redirect, draw.io UML actor and class (code committed 2ebe8f9).** Draw.io's `shape=umlActor` now exports with the palette's label styles. Composite cells with `attributes` and `operations` children import as the native Class shape, and native Class exports with the draw.io swimlane style and both children. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md records the added UML port.

**User redirect, draw.io pie slice (code committed f221136).** Draw.io's basic pie stencil now imports as the native Pie Slice, and native Pie Slice exports with quarter-turn angles. Imports with other angles report an approximation. Both integration tests failed before the fix and pass after it. THIRD_PARTY.md records Sidebar-Basic.js and mxBasic.js.

**User redirect, draw.io use case actor aliases (code committed c343695).** Draw.io's `mermaidUsecaseActor` variants now import as the native Actor. Hollow and silhouette types report a shape approximation, while native Actor export keeps the existing `umlActor` style. The integration test failed before the fix and passes after it. THIRD_PARTY.md records the added basic palette aliases.

**User redirect, draw.io ER cardinality arrows (code committed a6e408c).** Draw.io ER arrow names and `entityRelationEdgeStyle` now survive import and export on native connectors. Native heads approximate the combined markers, and imports report that approximation. The integration test failed before the fix and passes after it. THIRD_PARTY.md records mxER.js.

**Scope.**

- A selection context resolver that classifies the current selection as document, single
  shape, text layer, connector, frame, group, component, instance, or mixed selection.
- A Diagram tab for the empty selection with grid, background, paper, page view, and
  global style sections.
- Context gating for Style, Text, and Arrange tabs. A connector should not show text-only
  padding rows. A document should not show shape geometry.
- Page-scoped settings model: grid visibility, grid size, grid color, background color
  or image reference, paper size, orientation, and diagram-wide shadow and sketch
  defaults. Page view is a session-only view option because draw.io exposes it from the
  View menu rather than the page file.
- Move the existing Canvas section out of its always-visible footer position and into the
  Diagram tab, while keeping the command palette entry.
- Collapsible sections with remembered open state. Section state is UI state, not
  document data.
- Keyboard navigation and visible focus for the panel.
- Migration of existing pages to the new settings defaults. Existing files must load
  unchanged.

**Non-goals.**

- Global style schemes. STY-21 landed later in the working tree as a scoped,
  undoable restyle command, but it is not part of the M1 sidebar baseline.
- Adaptive colors. STY-22 landed later in the working tree, but it is not part of the
  M1 sidebar baseline.
- Font family. Text font selection is a separate text feature (TXT-06).
- Connector labels. They start in M2 and only get their inspector section here if the
  label model is already committed.

**Dependencies and constraints.**

- M1 depends on the native document model (ADR-001) and page-scoped settings decision
  (ADR-002).
- Settings changes must join the existing history transaction model. Grid visibility
  should be undoable when changed from the panel, just like a shape edit.
- Existing app-global grid size in `options.json` becomes a default for new pages, not
  the source of truth for saved pages.
- The panel must not become a single 2,000-line function. Split the context resolver,
  section builders, and control primitives before adding new rows.

**Exit criteria.**

- The selection matrix is covered by tests: empty, rectangle, text, connector, frame,
  group, instance, and mixed selection.
- Two pages keep different grid, background, and paper settings after switching, saving,
  and reopening.
- Existing v3 pages load with defaults and round-trip without changing content.
- A global restyle cannot be triggered accidentally from a style row.
- Screenshots at desktop width show Diagram, Style, Text, and Arrange states with no
  clipped rows and no horizontal overflow.

**Inventory rows closed or advanced:** UIX-01, UIX-02, UIX-03, UIX-04, DOC-08, DOC-09,
DOC-10, DOC-11, DOC-12, DOC-13, CAN-04, CAN-05, CAN-06.

## M2: Connector labels and routing polish

**Purpose.** Connectors are already bound and routed. They cannot carry a label, which
blocks ER diagrams, BPMN, flowcharts with yes and no branches, and most technical
diagrams.

**Working tree (2026-10-25).** Both steps are implemented on
`agent/drawio-features`: one edge-owned label per connector, centered at the midpoint of
the routed path for straight, curved, and elbow routes, and movable along or off that
route. `label_pos` stores the normalized distance along the flattened route and
`label_offset` stores a world-space displacement; dragging anchors the label to the
closest route point and stores the rest as offset, so it can sit on the line or away from
it and still follow a reroute. Double-click edits it on the canvas, the Text tab edits
text and typography, hit testing includes the measured label box, and save, load, undo,
clear formatting, and PNG/SVG/PDF export carry the label. Arrange > Reset has Label
Positions and Waypoints, each one undo step; both are disabled when nothing qualifies.

The transparent line gap also landed: a label without a solid background cuts the line
behind its text (see ADR-008). The cut is derived from the label layout, so it follows
moves and reroutes with no new stored state, and it reaches the canvas, sketch mode, and
SVG, PDF, and PNG export through the shared draw ops.

Reverse connector also landed (see ADR-008): the Arrange tab Reverse button swaps each
selected connector's source and target bindings and reverses its waypoints in one
`Reverse connector` transaction, matching draw.io's `turnShapes`. Arrowheads and labels
stay bound to the line's own ends, so they visually move to the opposite end while the
path stays put.

Line jumps also landed (see ADR-015): each connector stores a jump style (`none`, `arc`,
`gap`, `sharp`, `line`) and a size with draw.io's defaults, the Style tab gains a Line
Jumps select and size input, and a paint-order walk feeds each connector the points where
it crosses a connector painted below it. The hop shapes and half-gap formula follow
`mxConnector.paintLine`, hops compose with the label gap cut, survive sketch mode, and
reach SVG and PDF export.

**Working tree (2026-10-27).** Multiple labels per connector also landed (see ADR-008):
the first label stays in the existing fields and extras live in a per-connector label
list that inherits the connector's label style. An `Add label` button in the Text tab
creates one, each label drags along or off the line with its own position and offset,
double-clicking a label edits that label, and every label cuts the line behind its text
when it has no solid background. Reset Label Positions covers extras, and save, load,
undo, and PNG/SVG/PDF export carry the whole list.

Auto-route around obstacles also landed (CON-21): the elbow router now treats every
bindable, visible shape near the route as an obstacle, not only the two bound shapes.
Obstacles are the shapes' world boxes inflated by a fixed clearance, culled to the
route's search neighborhood so the A* grid stays bounded, and their edges become grid
lines the search can path around. A shape moved into a connector's route neighborhood
reroutes that connector in the same transaction, so the avoidance is one undo step with
the move. Straight and curved routes are unchanged, matching draw.io where only
orthogonal routing avoids shapes.

**Working tree (2026-11-13).** Fixed connection points also landed (CAN-15): hovering a
bindable shape, or dragging a connector end over one, draws small crosses at the four
corners and four edge midpoints, replacing the four circles that only appeared while
connecting. A dropped end within 12 screen pixels of an anchor stores that anchor's
normalized fraction instead of the raw pointer point, and both end drags and new arrows
use the same wrapper. The stored fraction is what keeps a snapped corner attached when
the shape moves or resizes; the round's runtime check resized a bound target from
160x100 to 240x140 and measured no pixel change around the anchored corner.

**Working tree (2026-11-14).** Rulers and manual guides also landed (CAN-16): a
page-level Rulers toggle draws 18 px strips along the canvas top and left with 1-2-5
tick spacing and labels, and dragging out of a ruler creates a guide. Dragging the line
moves it, dropping it on a ruler deletes it, and Clear Guides empties the list; each
gesture is one undo step and a drop on the ruler records Delete guide. Manual guides
outrank center alignment, equal spacing, and grid snapping for moves, and a resize snaps
only the dragged edge to the nearest guide. Guides paint in the canvas overlay and the
whole set persists in the page file.

**Working tree (2026-11-17).** Per-shape connector defaults also landed (CON-19). A shape
stores the style used by connectors started from it. The Style tab copies a selected
connector's style to the shape its line starts from, and the Arrange tab copies the page
connector default to selected shapes or clears their source default. Each command is one
undo step. New arrows, including quick-connect arrows, use the source default before the
page default, while existing connectors keep their own style. The value travels in the
page file and the draw.io UserObject sidecar, and ADR-008 records the extension.

**Working tree (2026-11-18).** Flow animation also landed (CON-16). A node stores an
optional flow setting with enabled, duration, timing, and direction. The renderer advances
a dash offset from the editor clock and rebuilds flowing connector meshes every frame.
Solid connectors get a simple long-short pattern when flow is on; authored dashes keep
their lengths. The Arrow section in the inspector edits all four values in one undoable
step. draw.io style keys map in both directions, including the older flowDuration key.
Exports and saved documents keep authored dash values, so only the interactive canvas
moves.

The M2 scope below is now fully implemented.

**Scope.**

- Edge-owned label data: text, style, normalized position along the route, offset from
  the route, and an optional background mode.
- One label per connector first, then a list if the model does not make the first case
  awkward.
- Double-click a connector to create or edit a label. Drag the label along the route.
  Drag it off the route with a visible leader or reset point.
- Label layout on straight, curved, and elbow routes. Transparent label backgrounds cut
  the line behind the text. Solid backgrounds paint a rounded box.
- Reset Label Position and Reset Waypoints commands.
- SVG, PDF, and PNG export, including label backgrounds.
- Reverse connector, line jumps, and multiple labels as small follow-ups if the route
  model is stable. All three have landed.

**Dependencies.**

- M1 for the Text or connector inspector section.
- ADR-008 for the edge-owned label model.
- The existing route geometry in `src/connector.rs`. No new graph layout engine.

**Exit criteria.**

- A labelled connector survives move, resize, rebind, route change, save, and reload.
- Label hit testing and editing work at 50% and 200% zoom.
- Export output places the label at the same relative point as the canvas.
- Reset Label Position is one undo step.
- No label is stored as a fake child shape that breaks when the connector is rebound.

**Inventory rows:** CAN-15, CON-09, CON-10, CON-11, CON-12, CON-15, CON-18, CON-21.

## M3: Layers

**Purpose.** Sketch has a z-order tree. draw.io layers are a separate page-level
organization with visibility and lock. Conflating the two would make both worse.

**Scope.**

- Page-scoped layer records: id, name, visible, locked, order.
- A layer membership field with inheritance. A subtree has one effective layer, separate
  from its parent group or frame. Moving a selection writes the chosen layer to every
  node in the subtree. Connectors may bind to nodes on other layers.
- Default layer for every existing page and a migration that puts all current content on
  it.
- Layers dialog: add, rename, duplicate, delete, reorder, show, hide, lock.
- Move selection to layer. Select all on layer.
- Paint order: layer order first, then the existing parent and sibling order inside a
  layer.
- Connector endpoints can cross layers without changing their binding.
- The Layers panel remains the z-order tree. The new dialog is for layers. Label both
  clearly so users do not confuse them.

**Working tree (2026-10-27).** The whole M3 scope is implemented on
`agent/drawio-features` (see ADR-009). Each page starts with a Background layer and old
pages get one through the v4 to v5 migration. A node stores its layer id and children
inherit the nearest ancestor's, so one subtree paints on one layer; a selection move
writes the layer across whole top-level subtrees, groups and frames included. Layers are
paint-order buckets over the unchanged root list: layer order first, then the existing
sibling order inside each layer, and the canvas, exports, and hit testing all walk the
bucketed order. The Layers dialog covers add, rename, duplicate (contents copied with
fresh ids above their source), delete with its shapes (the last layer refuses), reorder,
visibility, lock, select all on layer, and move the selection to a layer, each one undo
step that also restores the layer list and next id. Connectors keep bindings across
layers. MCP gets `list_layers`, a `layer_ops` batch tool, a layer argument on
`create_shape`, and layer fields on `get_node_info`. The z-order Layers panel is
untouched and told apart from the layer list. DIA-12 now uses custom links for
interactive layer actions.

**Dependencies.** M1 page settings and M2 label membership. The paint order change needs
tests for hit testing, clipping, and export.

**Exit criteria.**

- Hiding a layer hides its shapes, children, labels, and connectors, and removes them
  from hit testing.
- Locking a layer blocks selection and editing without changing per-node lock.
- Reordering layers changes paint order but never reparents a shape.
- A `.drawio` file with layers can be represented after M5 without inventing a second
  model.
- Existing pages open on a default layer with identical rendering.

**Inventory rows:** ORG-13, ORG-14, ORG-15, DIA-12.

## M4: Shape registry, search, and libraries

**Purpose.** The closed `NodeKind` enum covers the core canvas well. draw.io's breadth
comes from a searchable shape catalog. Sketch needs a native registry that can describe
domain shapes without importing mxGraph shape XML.

**Scope.**

- A native shape descriptor: category, name, keywords, preview, geometry builder, default
  style, and allowed connection points.
- Registry search with fuzzy matching and visual results.
- Categories for flowchart, UML, ER, network, floor plan, org chart, and containers.
- Library items can reference a registry shape or remain plain groups.
- Scratchpad as a lightweight recent and pinned area.
- Custom shape authoring is out of scope. Imported custom shape XML is an adapter
  concern for M5, not a reason to make the runtime model dynamic.
- No vendor icon packs. AWS, Azure, SAP, Citrix, Salesforce, and Veeam stay blocked until
  a license review.

**Working tree (2026-11-05).** M4 is implemented on `agent/drawio-features` (see
ADR-010). The registry is a static table of 30 entries across six categories: flowchart,
UML, ER, network, floor plan, and org chart. An entry carries a stable id, name,
category, keywords, preferred size, and a plain-style flag; it emits an existing native
kind or a path built from the vector primitives, so no new node field or runtime
interpreter was added. Plain entries take the page's remembered shape default on insert
and in their search preview, which is how a searched shape matches the current diagram;
stroke-art entries (actor, door, window, stairs) keep their own pen look. The Library tab
now has a fuzzy search field with a clear button and visual tiles, a scratchpad of pinned
and recent shapes, and the existing personal library, all sharing one tile with cached
vector previews. A tile click inserts at the viewport center, and dragging a tile onto the
canvas shows a ghost preview and inserts at the drop point. Double-clicking empty canvas
with the Select tool opens quick add at that world point; typing filters the registry and
Enter inserts the first match. Insertion is one undo step, parents into the frame under
the point or an explicit parent, and can assign the whole subtree to a page layer.
Autosize in the Arrange tab and on Alt+F runs deepest-first, hugs nested groups to their
children, gives frames 20 px of headroom, and keeps child world positions fixed. MCP gains
`search_shapes` and `insert_shape` with optional layer and parent arguments. Unit, editor,
and MCP tests cover the registry build, search, insert semantics, style parity, scratchpad
capping, and autosize. Custom shape XML, public galleries, a parametric shape editor, and
a generic property bag stay out of scope as documented below.

**Working tree (2026-11-12).** The command palette is now a search omnibox (see the
ADR-010 and ADR-013 extensions). The same Ctrl+K field ranks commands, tools, pages,
layers, templates, generators, and library items beside every registry shape, which it
labels as an insert entry with its category, and diagram text from every page. Choosing a
shape inserts it at the viewport center in one undo step. Choosing a text hit reuses the
find engine's fields, matching rules, labels, and previews, switches to that page, selects
the node, and zooms to it. Text results cap at 40 and appear only for a non-empty query.
The UIX-11 inventory row keeps its baseline status.

**Dependencies.** M1 for the panel and M3 for layer-aware insertion. A registry shape
must be able to say which layer a new instance joins.

**Exit criteria.**

- Searching "cylinder" or "queue" returns a shape and inserts an editable node.
- A registry shape can be saved to a library and inserted with fresh IDs.
- Preview rendering uses the same geometry as the canvas.
- New shapes participate in styles, connectors, frames, auto layout, and export.
- No shape definition can execute code or fetch a remote asset.

**Inventory rows:** CUS-05, CUS-06, CUS-07, CUS-08, CUS-09, CUS-10, CUS-11, CUS-16,
SHP-17; supports GEN-07 and GEN-08.

## M5: draw.io import and export adapter

**Purpose.** Interoperability is the main reason to care about draw.io formats. The
adapter must be honest about what it cannot represent.

**Scope.**

- Read `.drawio` and `.xml` pages, cells, geometry, labels, styles, edges, groups, and
  layers into the native model.
- Map a documented subset of styles: fill, stroke, dash, opacity, rounded corners,
  text alignment, and the shape kinds the registry supports.
- Import result with counts: imported, approximated, dropped, with reasons.
- Export the native model to `.drawio` XML for the supported subset.
- Preserve page names and order.
- Refuse or warn on encrypted or compressed payloads rather than producing a partial
  file silently.
- VSDX, Gliffy, and Lucidchart remain blocked until a separate parser decision.

**Working tree (2026-11-08).** M5 is implemented on `agent/drawio-features` (see
ADR-011). The adapter parses `mxfile` and `mxGraphModel` XML with a real XML parser into
an intermediate cell list, decodes percent-encoded and deflated diagram payloads, and
refuses encrypted files with a message naming the diagram. Import preserves page names
and order, maps layer cells to native page layers with cell 1 as Background, translates
parent-relative geometry into world and child-local rectangles, normalizes groups to the
union of their children, maps swimlane and container cells to frames, turns edges into
native connectors with bindings, waypoints, arrowheads, dash, and extra edge labels, and
resolves the common draw.io shapes through the native registry. The supported style
subset covers fill and gradient, stroke and width, dash, opacity, rounded corners and arc
size, rotation, text color, size, emphasis, alignment, and vertical alignment.
Unsupported cells and style keys become approximations or drops with a reason. Import
returns counts plus warnings, approximations, and drops. The app shows the report in a
dismissible window after opening or dropping a file, appends imported pages to the
current project while keeping their layers, and creates a project from the home screen.
Export writes every page into one uncompressed `mxfile`, from the inspector format list
(which forces all pages), the CLI `--export-drawio` and `--format drawio` flags, or the
export API. The checked-in fixture `tests/fixtures/drawio/two-pages.drawio` covers two
pages, a layer, a group, a swimlane, mixed styles, and an elbow edge with an extra label;
geometry round-trips within 0.01 world units. Pen paths and freehand strokes export as
basic shapes or edges, which stays a documented limitation.

**Working tree (2026-11-18).** DOC-15 is now partly implemented on
`agent/drawio-features` (see the ADR-011 extension). Native export can place a
versioned page bundle in SVG metadata or a PNG tEXt chunk. The bundle records the
project name, active page id, every page with its camera, and the original bytes of
referenced image assets. It is opt-in through the Export tab's Embed pages for
reopening checkbox and the CLI's --embed-data flag. Import detects the carrier
before the vector importers, so a file exported with data becomes a new project
with native pages instead of imported vector shapes. Files exported without the
option keep their existing carriers. PDF, HTML, and draw.io-native embedding stay
out of scope.

**Working tree (2026-11-18, round 32).** STY-22 extends the adapter with adaptive
colors. Import reads the page's `adaptiveColors` mode and `light-dark(light,dark)`
pairs for page background, fill, stroke, gradient stops, text, label background, and
label border. Export writes those values back. PNG and SVG export bake the appearance
the caller asks for, and the CLI exposes `--appearance light|dark`. A saved project
keeps both values, so changing the export appearance does not rewrite authored colors.

**Dependencies.** M2 for labels, M3 for layers, M4 for shape mapping. ADR-011 for the
adapter boundary.

**Exit criteria.**

- A checked-in fixture with two pages, groups, layers, labels, and mixed styles imports
  and exports with a documented fidelity report.
- Geometry round-trips within a stated tolerance.
- Unsupported cells are reported, not dropped without a trace.
- Exported XML opens in draw.io and shows the same page count and supported shapes.
- The importer never writes outside the new project folder.

**Inventory rows:** IO-15, IO-16, IO-17, IO-18, IO-21, DOC-15.

## M6: Diagram data, links, tags, and find

**Purpose.** Once shapes can carry data and text can be found, the editor becomes usable
for technical diagrams rather than only drawing.

**Scope.**

- Node data sidecar with stable key ordering.
- Edit Data dialog and an inspector section for properties.
- Custom links with a confirmation step before opening an external target.
- Tooltips and tags.
- Find across labels, tags, tooltips, and properties, with match stepping and optional
  regular expressions.
- Find and replace with one undo transaction per replacement batch.
- Placeholders for shape and page properties. Predefined date, page number, and page
  count values follow.
- No animated actions yet. Those depend on a viewer.

**Working tree (2026-11-10).** M6 is implemented on `agent/drawio-features` (see
ADR-013). Nodes now carry an ordered metadata map, a custom link, a tooltip, and a tag
list, all optional and omitted from the page file while empty. The inspector Data tab
edits the link, tooltip, tags, and shape properties, and the generalized Edit Data dialog
edits shape or page properties with add, rename, and remove rows. A page context supplies
the page name, number, count, and file name at render time; placeholders resolve from the
node's own metadata, the nearest ancestor's metadata, and then page metadata, with the
documented date and page values. Unresolved names render as a visible placeholder and a
double percent sign escapes a literal percent. Resolution runs in the canvas, SVG, and
PNG/PDF export, so the saved text stays a template. Custom links follow with Cmd/Ctrl
click or the Data tab Open Link button; page targets switch pages and external targets
are limited to http, https, and mailto and open through the platform opener after a
confirmation dialog. Find opens from Cmd/Ctrl+F and searches labels, extra connector
labels, text and sticky content, frame names, tags, tooltips, and metadata keys across
pages, with case sensitivity, optional regular expressions, and next and previous
stepping. Replace runs one undo step per page, skips locked and hidden nodes, and reports
the counts. Tooltips appear after a short hover. The draw.io adapter reads and writes
UserObject metadata and a sketch-data sidecar, so the checked-in metadata fixture
round-trips its properties, link, tooltip, and tags. Find and replace, link activation,
placeholder precedence, tag parsing, and the adapter round trip are covered by tests.
The rfd external-link confirmation could not be exercised under bare Xvfb without a
desktop portal; its request path and the unsafe-scheme refusal are covered by tests.

**Dependencies.** M5 for import and export of metadata. M1 for the inspector section.

**Exit criteria.**

- Metadata survives native save, export, and reimport.
- Find works across pages and reports an empty result clearly.
- Replace is undoable and never edits a locked or hidden node without saying so.
- A missing placeholder value renders a visible fallback, not an empty string.

**Inventory rows:** DIA-01, DIA-02, DIA-03, DIA-04, DIA-05, DIA-06, DIA-07, DIA-08,
DIA-09, DOC-14.

## M7: Templates and diagram packs

**Purpose.** A searchable shape registry is not enough. Users need complete starting
points and domain packs.

**Scope.**

- Native template format with pages, thumbnails, names, and categories.
- Template browser with preview and insertion.
- Custom template libraries from a local folder.
- Diagram packs for UML, ER, BPMN, network, floor plan, org chart, kanban, timeline,
  Gantt, C4, SysML, charts, and Venn diagrams, built only from native shapes.
- No vendor icon assets. Cloud stencils remain blocked.

**Working tree (2026-11-11).** M7 is implemented on `agent/drawio-features` (see
ADR-010). A template file is a validated JSON library with a format name and version,
holding named templates with a category, description, keywords, optional SVG thumbnail,
and one or more pages. A page stores ordinary nodes, root order, diagram settings, and
layers, so insertion produces editable native content rather than a picture. A node may
record the registry shape it was built from; if this build no longer has that shape,
insertion names it, leaves only that node out, and repairs bindings, child references,
and instance links so the rest still arrives. Fifteen bundled packs cover flowchart, UML
class, ER, BPMN, network, floor plan, org chart, kanban, timeline, Gantt, C4, SysML,
charts, Venn, and a three-page project plan, all assembled by pure builders from native
kinds and registry shapes. The browser opens from the Library tab or the command
palette, groups templates by library, filters by search and category, previews a page as
vector content, and either inserts one page on the open page or adds every page as new
pages. Templates join the page under one undo step, are centered where the pointer is
not given, and keep a layer when its name matches a page layer, otherwise they land on
the background. A custom folder chosen in the browser is saved to options, every JSON
library in it loads beside the bundled packs, unreadable files are reported and
skipped, and the current page saves back as a template with a thumbnail. The MCP server
gains `list_templates` and `insert_template` with page and page-set modes. Tests cover
the file format, every pack building a valid document, missing-shape repair, folder
loading, insertion undo and layering, the browser rendering, the palette entry, and the
MCP tools. Runtime checks on the release build inserted a BPMN pack, undid and redid it
as one step, added the three-page project plan as pages, loaded a hand-written custom
library from the options folder, reloaded it from the browser when the first scan had
reported a bad file, inserted it from the dialog, and saved the open page back as a
template.

**Dependencies.** M4 for registry shapes, M6 for metadata and labels.

**Exit criteria.**

- A template inserts as editable native content, not a flattened image.
- A template with a missing registry shape reports the missing shape and still inserts
  the rest.
- Every pack has at least one round-trip export test.
- No template carries an unlicensed asset.

**Inventory rows:** GEN-06, GEN-07, GEN-08, GEN-09, GEN-10, GEN-11, GEN-12, GEN-13,
GEN-14, GEN-15, CUS-12, CUS-13, CUS-14.

## M8: Data and text generation

**Purpose.** draw.io can turn Mermaid, CSV, SQL, and JSON into diagrams. Those are useful
without any hosted model and should be considered separately from AI generation.

**Scope.**

- Mermaid flowchart and sequence import.
- CSV import with a small set of layouts.
- SQL schema import into ER tables.
- JSON graph import with a deterministic layout.
- AI generation is blocked. It would require a hosted model, credentials, a network
  path, and a privacy policy that conflicts with the local-first default.

**Working tree (2026-11-11).** The four local generators are implemented on
`agent/drawio-features` (see the ADR-011 extension). A new generator module parses text
into an ordinary document, then the editor inserts that document through the same
subtree path as library and template content, either on the open page in one undo step
or as a new project page. Nothing about a generated node is special: selection, styling,
bindings, layers, autosave, and export work as they do for hand-drawn content.

Mermaid import reads flowcharts and sequence diagrams. Flowcharts accept node shapes,
labels, subgraphs, and the common solid, dashed, thick, and hidden edge forms with pipe
labels or inline text, plus self-loops; sequence diagrams become participant boxes,
lifelines, and labelled messages in declaration order. Unsupported directives are
reported as warnings instead of failing the import. CSV import detects comma, semicolon,
tab, or pipe delimiters, honours quoted fields with escaped quotes and embedded newlines,
and offers grid, vertical and horizontal flow, and vertical and horizontal tree layouts.
A header row names the columns, a chosen one-based label column picks each shape's label,
and tree layouts read the first cell as the row id and the second as its parent. SQL
schema import turns each CREATE TABLE into an editable table frame with a title, a rule,
and one row per column, marking primary and foreign keys; column-level REFERENCES,
table-level FOREIGN KEY clauses, and ALTER TABLE ADD FOREIGN KEY statements become bound
connectors labelled with the local column. JSON graph import accepts nodes and edges with
from or source and to or target, optional labels, dashed or arrow flags, and common shape
names; per-node coordinates and sizes are kept only when every node has coordinates,
otherwise the graph gets a layered or grid layout. Dangling edges, duplicate ids, and
malformed rows are warnings with the row number, while a parse failure returns one clear
message that names the line or token.

Every source has a fixture and a failure case in its tests, and the same input produces
the same layout each time. The dialog opens from a new Insert menu and the command
palette, carries a source selector, the text area, CSV layout and header controls, and
shows parser errors in place without closing. The MCP server gains `generate_diagram`
with the same options and a result that reports the source, mode, page, node count, and
warnings. No generator path makes a network request. AI generation stays blocked for the
reason above.

**Dependencies.** M5 for ER tables and labels, M7 for templates.

**Exit criteria.**

- Each parser has a fixture and a failure case with a clear message.
- Generated diagrams are editable native objects, never images.
- Layout is deterministic for the same input.
- No network request is needed for the non-AI paths.

**Inventory rows:** GEN-01, GEN-02, GEN-03, GEN-04, GEN-05.

## M9: Collaboration and integrations

**State: blocked.** Cloud storage, real-time collaboration, share links, comments,
revision history, Confluence, Google, Microsoft, VS Code, Notion, WordPress, and plugins
all need hosted services, identity, permissions, or a plugin sandbox. None of that is
scheduled. Export files and the local project folder are the supported sharing path.

The blocker is not a missing button. It is the absence of a server, a conflict model, a
permission model, and a reason to operate them. Revisit only with an explicit product
decision.

**Inventory rows:** STO-02 to STO-11.

## Dependencies at a glance

```text
M1 format sidebar + page settings
  |
  +--> M2 connector labels
  |      |
  |      +--> M3 layers
  |             |
  |             +--> M4 shape registry
  |                    |
  |                    +--> M5 draw.io adapter
  |                           |
  |                           +--> M6 data, links, find
  |                                  |
  |                                  +--> M7 templates and packs
  |                                         |
  |                                         +--> M8 generation
  |
  +--> M9 collaboration and integrations (blocked, no dependency path)
```

M2 is not strictly blocked by M1 in code, but the label inspector belongs in the panel
that M1 establishes. Doing M2 first would add another set of special-case rows to the
current inspector.

## Ten concrete next tasks

These are written so an agent can start without reopening the whole roadmap. Each task
names the inventory IDs it advances and the acceptance check.

### T1. Add a selection context resolver

**Files:** `src/ui/inspector.rs`, a new `src/ui/format_context.rs` or equivalent.

**Work:** classify the selection into document, shape, text, connector, frame, group,
component, instance, and mixed. Return the applicable tabs and sections. Keep selection
reading out of the drawing code.

**Acceptance:** a unit test matrix produces the expected context for each selection kind.
A mixed selection that cannot show a section does not show it.

**Advanced:** UIX-02.

### T2. Split the inspector into section builders

**Files:** `src/ui/inspector.rs`.

**Work:** move Style, Text, Arrange, and the new Diagram sections into separate builders
with a shared row and section API. Keep existing control IDs stable so egui state does
not reset.

**Acceptance:** `cargo clippy` and the existing inspector screenshot tests pass. No
section builder is longer than the current style section.

**Advanced:** UIX-01, UIX-04.

### T3. Add the Diagram tab shell

**Files:** `src/ui/inspector.rs`, `src/ui/format_context.rs`.

**Work:** when the selection is empty, show a Diagram tab with a Grid section, a Page
section, and a Global style section. Keep the existing grid size and snap behavior
working during the move.

**Acceptance:** deselecting everything shows Diagram. Selecting any shape hides it.
Grid size still changes snapping and survives a save.

**Advanced:** UIX-03, CAN-05.

### T4. Add page-scoped diagram settings and migration

**Files:** `src/model/document.rs`, `src/project/format.rs`, a new settings module.

**Work:** add page settings with serde defaults. Bump the page format from v3 to v4 and
write a pure migration that inserts defaults. Keep app preferences in `options.json`.

**Acceptance:** old pages load with defaults, new pages save the values, and
`page_roundtrip_is_deterministic` still passes. A test asserts two pages keep different
values.

**Advanced:** DOC-12; enables DOC-08 to DOC-11.

### T5. Make grid settings page-scoped

**Files:** the settings model, `src/editor/mod.rs`, `src/render/scene.rs`,
`src/options.rs`.

**Work:** read grid visibility, size, and color from the active page. Use the app option
only as the default for a new page. Render the page color in both themes.

**Acceptance:** two pages with different grid sizes render differently after switching
and after reload. The grid color override survives a theme switch.

**Advanced:** DOC-12, CAN-04, CAN-05, CAN-06.

### T6. Add page background and paper controls

**Files:** page settings, `src/ui/inspector.rs`, `src/render/scene.rs`,
`src/io/export.rs`.

**Work:** background color, background image reference, paper size, orientation, and page
view. Render the page boundary when page view is on. Include the background in export
unless transparent output is requested.

**Acceptance:** an A4 portrait page shows the correct boundary, a background color
appears in PNG and PDF, and an image background is stored with the project.

**Advanced:** DOC-08, DOC-09, DOC-10, DOC-11.

### T7. Gate Style, Text, and Arrange sections by context

**Files:** `src/ui/inspector.rs`, context resolver.

**Work:** hide text-only rows for non-text selections, hide geometry rows for connectors,
and hide layer or component rows when they cannot apply. Do not disable a row without a
reason string.

**Acceptance:** a connector shows route and heads but no text padding. A text layer shows
text controls but no frame clip controls. A frame shows frame and auto-layout controls.

**Advanced:** UIX-02.

### T8. Add collapsible sections with remembered state

**Files:** `src/ui/inspector.rs`, `src/ui/mod.rs`.

**Work:** persist open and closed section state in `UiState`. Add a section header that is
keyboard reachable. Do not store section state in the document.

**Acceptance:** collapsing a section survives a restart. The panel screenshot shows a
consistent order.

**Working tree (2026-11-18):** the chevron writes the key to app options and marks
them dirty, missing keys default to expanded, and filtering forces matching
sections open without changing saved state. Headless egui tests render the panel
and check matching rows and the no-match message. Runtime in the release app: a
collapsed key survived a restart and one click expanded it again.

**Advanced:** UIX-04.

### T9. Make page settings undoable and autosaved

**Files:** `src/history.rs`, `src/session.rs`, page settings code.

**Work:** route settings changes through the existing transaction recorder. Autosave
marks the page dirty. Do not create a history entry for a no-op selection of the same
value.

**Acceptance:** changing grid size is one undo step; undo restores the previous size and
the file saves after the undo. A no-op change adds no history entry.

**Advanced:** DOC-12; protects all M1 settings.

### T10. Record panel evidence and close M1

**Files:** `tests/`, `docs/implementation/verification-log.md`,
`docs/screenshots/` only if the change is meant to update them.

**Work:** add the selection matrix, two-page settings round-trip, and an inspector
screenshot at desktop width. Record commands and outputs. Do not rewrite committed
screenshots unless the milestone is meant to change them.

**Acceptance:** the evidence log names each command, result, and artifact. M1 exit
criteria are all checked. Remaining gaps move to M2 with a reason.

**Advanced:** UIX-01 to UIX-04, DOC-08 to DOC-13.

## Risks and mitigations

| Risk | Why it matters | Mitigation |
|---|---|---|
| The panel becomes a monolith | Every later feature adds rows, and the file is already large | Split context, sections, and controls before adding M2 and M3 rows |
| Page settings leak into app preferences | Two pages would share grid or background by accident | Settings live in the page model; `options.json` only supplies defaults |
| Global restyle surprises users | One click could recolor an entire diagram with no clear scope | Explicit scope, preview, and one undo step; no restyle from a generic style row |
| Layer order conflicts with group order | Paint order, hit testing, and export can disagree | Define layer first, parent second, sibling third, and test all three |
| Import silently drops content | Users trust a file that is no longer complete | Required loss report and fixtures for every adapter |
| Proprietary assets enter the repo | Clean-room and licensing promises break | Vendor packs stay blocked; only native geometry and licensed fonts |
| Evidence is claimed but not run | The inventory becomes marketing | Status rules, named tests, and a verification log that lists what was not run |

## Definition of done for any milestone

1. The listed inventory IDs are implemented or explicitly deferred with a reason.
2. The behavior has a committed test or a recorded runtime check, plus export coverage
   when it affects output.
3. The format panel has a home for the new controls and does not add an ungrouped row.
4. File changes have a migration and a round-trip test.
5. The verification log records the commands, date, result, and remaining gaps.
6. No proprietary assets, network calls, or destructive global edits were added.

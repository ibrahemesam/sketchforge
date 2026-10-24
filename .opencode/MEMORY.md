# Sketch agent memory

Working notes for agents picking this project up in a later session. Keep this
file current when the task, environment, or verification state changes.

## ROUND 5 — DONE, committed on `agent/drawio-features` (see `git log -1`)

Remaining Text-tab controls from the draw.io screenshot: vertical Position,
Writing Direction, Background/Border color, Shadow, Opacity.

- New `Node` fields (NOT in `NodeKind`, same reason as `Pad`): `text_bg:
  Option<Color>` (Background Color — kept OFF `style.fills`, which is the FONT
  color for text layers), `valign: VAlign{Top,Middle,Bottom}`, `dir:
  WritingDir{Auto,Ltr,Rtl}`, `text_shadow: bool`. Serde `default` +
  `skip_serializing_if` (`is_false` for the bool). `model::mod` re-exports
  `VAlign, WritingDir`.
- Vertical Position only matters when the box is taller than content:
  `fit_text` (editor/mod.rs) keeps `rect.h` when `valign != Top` (Top
  re-fits); `text::text_origin` adds `dy = extra×{0,.5,1}` above `pad.top`.
- Writing Direction: `layout_node` routes to `layout_bidi` with
  `para_dir: Option<Level>` for any explicit dir or Arabic content. KEY
  TRICK: `unicode_bidi::BidiInfo::new(text, default_para_level)` only uses
  the default when the text has NO strong character, and mutating
  `paragraphs[].level` after construction does not affect `visual_runs`
  levels. Solution: prefix the line with a zero-width strong char (`\u{200F}`
  / `\u{200E}`) before `BidiInfo::new` and subtract it (`pre`) when mapping
  run byte ranges back to cells. "Hi Ally. Draw" is a bad visual witness
  (all neutrals sit between two L runs → identical layout under RTL); "Hi
  Ally." flips its trailing dot to the leading position.
- Renderer (draw.rs `node_draw` Text branch): `text_bg` → rounded Fill behind
  the box (`style.corner_radius`), `style.stroke` → Stroke border,
  `text_ops` emits a black shadow glyph pass (offset `size*0.07/0.10`, alpha
  96) before the colored `DrawOp::Glyphs`.
- Text tab rows (before Spacing in `text_section`): 3 valign icon buttons
  (AlignTop/VCenter/Bottom) + Position combo (80%, 70px), "Direction" combo
  (Automatic/LTR/RTL, 86px), Background checkbox + `color_button` (defaults
  white on toggle), Border checkbox + color button (INK, forces
  stroke_width 1.0), Shadow checkbox, Opacity row (percent ↔ `style.opacity`).
- Tests (153 lib): `valign_keeps_height_and_offsets_the_box`,
  `writing_direction_overrides_paragraph_bidi` (asserts RTL glyph order by
  `char_index`: dot = index 7 LEADS; also Auto == LTR by gid list).
- Runtime evidence (Xvfb :99, artifacts/): Text tab shows all rows
  (r5-texttab.png); Background → white band (g3), Border+Shadow (g4),
  Opacity 40 → dimmer (g5), Position Center + corner-resize to 160×150 →
  text vertically centered (g8, g13); RTL dot-flip ".Hi Ally" (h2, h3,
  h4crop). Cleaned up: text node deleted, LabelTest `440765:15` back at
  (0,0) #202020 240×140, no auto layout; Export Transparent background off;
  options.json grid_size 10.0.
- Pitfalls that ate time: xdotool typing capitals sends ⇧A, and ⇧A with a
  frame selected = Add auto layout → an invisible frame silently swallowed
  nodes (delete removes children). `get_children` output truncation: don't
  pipe to `head`. MCP writes don't repaint — jiggle the pointer before
  screenshotting. `set_text_content` = `{"updates":[{"nodeId",
  "textContent"}]}`. Small text nodes' mid-bottom handle drags register as
  MOVE; corner resize works; click twice after windowfocus.

Skipping still (honest): Font family (single bundled Inter), "Formatted
Text" (always on); earlier rounds: angle/rotation, Glass text effect, paper
size/page view/background image, Diagram tab.

## ROUND 4 — DONE, committed `599c76d` on `agent/drawio-features`

HEAD is now `831d128` → `d5b5234` → `599c76d`. All four rounds are committed;
nothing pushed. Round 4 (this session):

- Text spacing (draw.io Spacing): `Pad { left, top, right, bottom }` on
  `Node` (NOT inside NodeKind — avoids breaking every `NodeKind::Text {…}`
  literal), serde default + `skip_serializing_if Pad::is_zero`. `text::layout_node`
  insets the wrap column to `rect.w - pad.left - pad.right`; `fit_text` grows
  w (auto_width) and h by the pads; `text_origin` for Text nodes is
  `v2(pad.left, pad.top)` — explicit Sticky match branch FIRST (Sticky keeps
  STICKY_PAD; `NodeKind::text()` also matches Sticky, easy to regress).
  Text tab shows All (= max side, writes all four) / Top / Left / Bottom /
  Right rows in two 3-column lines, `vis` param added to `text_section`.
- Style tab: palette grid (PALETTE_COLORS 10, 5×2) above/through fills —
  click recolors topmost fill, creates one if none; Theme presets
  (THEME_PRESETS 8, 4×2, fill+line pairs; Connector takes line only);
  Rounded checkbox next to Radius in `geometry()` (Arrange tab; radius
  0↔6.0, clears per-corner `style.radii`).
- Canvas section (dot grid + size combo + Snap guides) under the tabs in
  both paths (selection + nothing-selected); `nothing_selected` lost its
  `st` param (its Grid row removed).
- Auto-layout section deleted (user: "remove this damn autolayout"):
  `fn auto_layout` + call site gone from inspector.rs; Repos still work,
  the panel just doesn't manage it.
- New tests (151 lib total): `text_padding_grows_box_and_offsets_content`,
  `stickies_keep_their_fixed_inset`.
- Runtime verified on Xvfb (screenshots in /tmp/opencode/artifacts/):
  palette click → computed bg #2b6fd8; Slate theme → fill #2a2f3a +
  1px white line; Rounded → Radius 0.0→6.0; Text pad Top 20 → box
  102.6×19.4 → 102.6×39.4 with text sitting below the top edge; frame
  Arrange tab has no Auto layout row.
- model::mod re-exports `Pad`. egui 0.36: `rect_outline` doesn't exist —
  use `rect(..., StrokeKind::Inside)`.

Skipping still (honest): Font family (single bundled face), vertical
Position align, Writing direction/RTL, angle/rotation, Glass/Shadow text
effects, Paper size/Page view/Background image, "Formatted Text" (always on).

### MCP schema facts (round 4)

- `get_node_info` needs `nodeId`, not `id`; shows no fill info — use
  `get_computed_styles {"nodeIds":[...]}` (bg, border, size, x/y).
- `update_styles` = `{"updates":[{"nodeIds":[...],"styles":{CSS-ish}}]}`;
  works: `background`, `border`, `border-radius`. Unknown keys come back
  in `ignoredStyles`. `border":"none"` removes the stroke.
- Text tool flow: `xdotool key t` may be eaten if window focus was just
  set — re-screenshot between steps. Frame drag needs slow multi-step
  mousemove (3-4 waypoints); single jumps don't create the frame.
- Palette swatch coords in the 1366-wide window: row1 y≈289 x=1131+29k,
  row2 y≈318; Theme cards y≈549/577. First click after `windowfocus` is
  often dropped — click twice.

## ROUND 2+3 — DONE (committed `d5b5234`)

Committed in this batch (see `git log -1` after commit):

- Grid fallback everywhere: move, resize (per dragged edge only, sym skipped,
  opposite edge fixed), create end + create start, arrow start, connector
  end, path point. Behavior matches mxGraph's `mxGraph.snap`
  (`round(v / gridSize) * gridSize`) and `mxVertexHandler` (only dragged
  edges snap; grid-off still pixel-rounds). Cmd/Ctrl (precision) and shift
  stay free; `snap::grid_v` helper added in `src/editor/snap.rs`.
- draw.io reference (behavior only, no code copied): `jgraph/mxgraph`
  master `javascript/src/js/{view/mxGraph.js,view/mxGraphView.js,handler/
  mxGraphHandler.js,handler/mxVertexHandler.js}` and `jgraph/drawio` dev
  `src/main/webapp/js/grapheditor/Graph.js` (fetched to /tmp/opencode/).
- New tests (149 lib tests total, 205 across all suites): 
  `creating_uses_grid_when_no_anchor_hit` (two_rects-style + far sibling,
  grid 20, end 228.4→220 / 170→180), `resizing_uses_grid_when_no_anchor_hit`
  (edge + corner; note the down point must be at the handle, e.g. (453,353)
  for a bottom-right corner of b(400,300,50,50), NOT inside the rect).
- Word wrap runtime verified live: wrap ON + narrowed box → 260×38.72
  (two lines); OFF → back to 336.73×19.36. Node 440765:10 used, then
  deleted during cleanup.
- Grid size survives open paths: explore agent found `session.open` →
  `load` resets `grid_step` to 10; fixed by syncing `ed.grid_step =
  st.options.grid_size` in both `App::new` Project arm AND `open_project`
  (covers Home open, New, import, SaveAs-duplicate; MCP open_project at
  src/mcp/exec.rs remains without it — acceptable, headless).
- Runtime evidence files: /tmp/opencode/artifacts/{r3-create.png,
  wrap-wrapped.png, reopened2.png, grid10.png, final-state.png} + agent
  run (text-tab.png, 03/04 bold+clear round trip).

## Grid-check project is RESTORED CLEAN (round 4)

Single layer LabelTest `440765:1`, fill #202020, no stroke, (0,0)
240x140, corner radius cleared; test Text node and Frame deleted;
/home/ibrahem/mnt/Files/Documents/sketch-rs/options.json `grid_size: 10.0`.

### Round-2 feature recap (all runtime verified)

Word wrap (Text tab checkbox → `set_word_wrap`), Clear formatting both
modes (whole-layer + range), Set Default / Reset Default Style (session
scope, non-frame only), Grid Size combo 5/10/20/50 (persisted via
options.json, clamped 2..=100 on load), Snap to Grid button honoring
`ed.grid_step`. Backed where the model supports it; vertical text align
skipped (boxes auto-fit height).

### LIVEAPP procedure (unchanged, works well)

- Xvfb `:99`, window id 2097154; restart:
  `p=$(ps -eo pid,args | awk '/debug\/sketch-rs \/home/ && !/awk/{print $1}'); kill $p; sleep 2`
  then `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE -u XAUTHORITY
  SKETCH_RS_MCP_PORT=29981 DISPLAY=:99 nohup ./target/debug/sketch-rs
  /home/ibrahem/mnt/Files/Documents/sketch-rs/grid-check >/tmp/opencode/app.log 2>&1 & sleep 7`
  then `xdotool windowsize 2097154 1366 768` + `windowfocus` before every input burst.
- Content offset zoom 1: world (0,0) ≈ screen (588,396); verify visually first.
- Build with grid 20 preloaded in options.json to make fallbacks observable.
- MCP helper: `python3 /tmp/opencode/mcp.py <tool> '<json>'`; labels used:
  get_children/get_node_info/get_selection/update_styles(left,top,width,
  height,background)/delete_nodes. MCP `left/top` is NOT plain world x/y —
  verify via get_node_info after each write (a left=20 landed around
  world 121 once; unknown mapping, empirical only).
- Selection geometry: read the selection outline from a screenshot rather
  than trusting computed positions (text-node right handle was at
  (1046,697) after a "left" write, not where arithmetic said).
- Word-wrap caveat: toggling wrap ON at full content width changes
  nothing visually (one line still fits); narrow the box first.
- `import -window root` for screenshots; user's own Sketch runs on :0
  (MCP 29980) — never kill or touch it.

## Remaining ideas (not requested, from draw.io parity)

- Grid section in a proper View menu entry? Currently lives in the
  no-selection panel only.
- Snap-to-Grid command could also size nodes to the grid (mxGraph
  `snapCellsToGrid` also fits widths/heights); ours only moves x/y.
- draw.io `snapDelta` tolerance (gridSize/2 dead zone) is unexplored.


## Current task (branch `agent/drawio-features`, all uncommitted)

Same checkout as before: `/home/ibrahem/mnt/Files/Desktop/Linux/Apps/sketch-rs`
(edits in place by user request; never touch files outside it).

| # | Change | Code state | Runtime check |
|---|---|---|---|
| 1–5 | Earlier draw.io features (drop, grid, dbl-click label, container, contrast) | done | verified |
| 6 | Cursor over selection resize handles: ⤢/⤡ corners, ⟷/↕ sides | done | unit tests only (no OS cursor probe available) |
| 7 | Ctrl+Left/Right word jump in canvas text; Shift selects by word | done | verified (select-highlight screenshots) |
| 8 | Escape on canvas text edit now KEEPS changes (user changed their mind; previously discard) | done | verified (MCP: node kept text) |
| 9 | Left-panel element rename: Enter commits+unfocus, Ctrl+Enter newline, Escape unfocus+discard; focus no longer pinned (root cause: per-frame `request_focus`) | done | verified (MCP: name set, newline name, name unchanged, click-away commit) |
| 10 | Inspector tabs Style \l Text \l Arrange in the right panel; Arrange = z-order buttons, X/Y/W/H with constrain-proportions lock, Copy/Apply Size, Flip H/V, Snap to Grid, Group/Ungroup, Lock/Unlock; Style gets Copy/Paste Style buttons | done | verified (screenshots style-tab-v3b/arrange-tab-v3; Snap to Grid 13.4→10, 27.6→30; Bring Forward + To Back via MCP order; constrain W240→H70; Apply Size copied 240x140) |
| 11 | Magnet resize inside and outside frames: `resize_bounds` now takes the resized ids (fixes hover-resize with empty `top_selection`) and returns `(Rect, Vec<Guide>)`; guides routed to `ed.guides` and kept even at `sx==0` (absorption state shows the hint line) | done | verified (unit test `resize_magnet_shows_guides_on_sibling_anchor`; user-visible guides were the FIXME) |
| 12 | Magnet create: `Gesture::Create` snaps the dragged END onto sibling anchors (start corner fixed) with grid/round fallback; inside frames it magnetizes to siblings | done | verified (tests `creating_magnetizes_to_sibling_anchors`, `creating_inside_frame_magnetizes_to_frame_children`; runtime: created rect right edge magnetized to x=240, red guide visible in create-magnet-mid2.png) |
| 13 | Marquee inside frames recurses into grazed frames (`marquee_collect`) | done | verified |

Final checks pass: `cargo fmt --check`, `git diff --check`,
`cargo clippy --all-targets -- -D warnings`, full `cargo test` (143 lib +
integration suites). `docs/screenshots` restored afterward. `cargo build
--release` done per user request (17.6s, clean).
Changes remain uncommitted on `agent/drawio-features`; ask the user before
committing (their earlier guidance).

### Inspector width pitfall (egui)

`Panel::right` wraps the inspector in a Frame with 14px side margins; the
ScrollArea's `clip_rect` extends into the margins (clip right = window edge),
so `ui.clip_rect().right()` over-reports. Also, any row wider than the panel
inflates the content rect so later `ui.available_width()` shrinks/overflows.
Fix: capture `let vis = ui.available_width()` ONCE at `panel()` start (before
any content) and pass it down to `tabs`, `style_actions`, `arrange_section`;
two-button rows also set `item_spacing.x = 4.0` and size `w = (vis - 4)/2`.

## Environment / how to drive the app headlessly

- **Xvfb is NOT guaranteed to be running.** Start it first:
  `Xvfb :99 -screen 0 1366x768x24 -nolisten tcp -ac &`
- Launch a test instance:
  `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE -u XAUTHORITY SKETCH_RS_MCP_PORT=29981 DISPLAY=:99 nohup ./target/debug/sketch-rs <project-path> >/tmp/opencode/app.log 2>&1 &`
- **The user's own Sketch runs on display `:0` (PID visible via `ps -eo pid,comm`,
  comm `sketch-rs`) and owns MCP port 29980. Never kill it.** Test instances must
  set `SKETCH_RS_MCP_PORT` (e.g. 29981) or the MCP server silently fails to start
  (warning in app.log: "port 29980 is in use").
- pgrep gotchas: `pgrep -f sketch-rs` matches your own shell (watch for stray
  "Killed by SIGTERM"); identify instances by comm `sketch-rs` + `/proc/*/environ`
  DISPLAY value instead.
- Test project used: `/home/ibrahem/mnt/Files/Documents/sketch-rs/grid-check`
  (agent-created; fileId `40e8748c29e6df44`, page `4407658a4075770f`, rect node
  `440765:1` 240x140 at world 0,0 named "LabelTest"). Restore its state when done
  (rename via `rename_nodes` with `updates:[{nodeId,name}]`).
- **Re-focus the window after every app restart**: `xdotool windowfocus <id>`,
  else synthetic keys are silently dropped.
- xdotool modifiers: `xdotool key ctrl+Return` delivers NO modifier to winit;
  use `xdotool keydown ctrl; xdotool key Return; xdotool keyup ctrl`.
- Screenshots: `DISPLAY=:99 import -window root /tmp/opencode/artifacts/x.png`.
  The canvas is dark; dark `Color::INK` text on it is nearly invisible — rely on
  selection-highlight bands and MCP node data instead of glyph pixels.
- MCP helper: `python3 /tmp/opencode/mcp.py <tool> '<json args>'`. Recreated this
  session; parses SSE properly (tool args must match the schema, e.g.
  `node_info` needs `nodeId`, `rename_nodes` needs `updates`). Camera math at
  100% zoom: screen = world + (590, 397) in this window size.
- Double-click to edit canvas text: two `xdotool click 1` ~250ms apart works;
  `click --repeat 2 --delay 120` sometimes registers only a selection.

## Implementation map

- `src/editor/snap.rs` — grid snapping (10 units).
- `src/editor/shortcuts.rs` — `C` shortcut for Container.
- `src/mcp/exec.rs` — MCP `node_info` label output and `set_text_content`.
- `src/render/draw.rs`, `src/render/scene.rs` — label draw ops, grid lines, caret overlay.
- `src/text.rs` — `layout_node` lays out labels; `text_origin`; `label_color` auto-contrast.
- `src/model/node.rs` — `Node.label`, `label_color`, `container`; text helpers
  (`edited_text`, `edited_text_and_style_mut`), `supports_label`.
- `tests/headless.rs` — integration suites.

## Implementation map (new entries)

- `src/editor/input.rs` — new `CursorShape { NwSe, NeSw, Ew, Ns }` +
  `CursorShape::resize(handle)`; `hit::HANDLES` fractions (0,0)/(1,1)→NwSe,
  (1,0)/(0,1)→NeSw, x==0.5 sides→Ew, y==0.5 sides→Ns.
- `src/editor/mod.rs` — `Editor::hover_cursor()` (gestured + hovered handle,
  gated on Tool::Select and no text edit); `TextEdit.original` field REMOVED
  (Escape no longer discards).
- `src/app.rs` — `redraw()` computes `hover_cursor` before taking the gfx
  borrow, then overrides `l.output.platform_output.cursor_icon` with the egui
  variants (ResizeNwSe/NeSw/Horizontal/Vertical). egui-winit re-applies its own
  cursor every frame, so the override must happen per-frame there.
- `src/editor/text_edit.rs` — `word_caret(chars, from, forward)` (word =
  alphanumeric/_/'; punctuation runs count as words; forward also jumps the
  trailing whitespace); Left/Right arm prefers ctrl/alt → word move, then cmd
  line-end behavior.
- `src/ui/mod.rs` — `UiState.renaming_focus` (one-shot focus flag); layer-row
  rename now: Escape→close discard, Ctrl+Enter→insert `\n` at egui caret
  (`TextEditState.cursor.char_range()`, `CharIndex(pub usize)`) with manual
  cursor advance, Enter/click-away→commit via `lost_focus`.
- `src/ui/pages.rs` — same one-shot focus + Escape-discard for page rename.

## Implementation map (drawio session, this checkout)

- `src/ui/inspector.rs` — tabbed `panel()` (`InspectorTab` + `st.inspector_tab`);
  `tabs()`, `arrange_section()` (z-order/geometry/flip/snap/group/lock),
  `style_actions()` (Copy/Paste Style), `geometry()` gains the aspect lock via
  `st.lock_aspect` / `st.aspect_ratio` and Copy/Apply Size via `st.copied_size`.
- `src/ui/mod.rs` — `UiState`: `inspector_tab`, `lock_aspect`, `aspect_ratio`,
  `copied_size`; `INSPECTOR_TAB` order Style→Text→Arrange.
- `src/editor/mod.rs` — `resize_bounds(ids, ...) -> (Rect, Vec<Guide>)`;
  Resize arm sets `self.guides` from it; `Gesture::Create` magnet block;
  marquee `marquee_collect` recursion into grazed frames.
- `src/editor/snap.rs` — `snap_rect` axis masks; used by move/resize/create.
- `src/editor/tests.rs` — `resize_magnet_shows_guides_on_sibling_anchor`,
  `creating_magnetizes_to_sibling_anchors`,
  `creating_inside_frame_magnetizes_to_frame_children` (magnet threshold is 6.0
  world units; endpoints in tests must land within it).

## Decisions and behavior

- Grid: square lines every 10 world units, stronger lines every 50; moves snap
  to 10 when object/guide snapping does not apply.
- Labels: shapes and other non-text nodes keep text in `Node.label`; text nodes
  keep it in the kind. Double-click enters edit. Empty labels are harmless.
- Container: new `Tool::Container` (`C`) creates a `Frame { clip: false }`,
  320x220 default, transparent fill, dashed border. Shapes drawn inside are
  adopted as children. Dragging the container body moves children (child rects
  are parent-relative).
- Auto contrast: WCAG relative luminance; white below 0.45, otherwise
  `Color::INK`. Applies to shape labels and sticky-note text.
- Escape everywhere in text editing now keeps typed changes; an empty Text node
  is removed by `end_text_edit` on the way out.
- Rename fields: focus is requested exactly once when the rename opens.
- Cmd/Ctrl+Backspace inside canvas text still deletes to line start (kept).

## Verification log (latest session)

- Cursor feature: `cargo test --lib` covers `hover_cursor()` for all 8 handles,
  no-selection, mid-box, and during a resize drag (`Gesture::Resize`). The
  os-level icon check was skipped: python-xlib is missing and nothing outside
  sketch-rs may be installed; the egui→winit mapping is egui's own standard
  table (ResizeHorizontal→EwResize etc., confirmed in egui-winit 0.36 source).
- Word jump: unit test `ctrl_arrows_jump_over_words_and_shift_selects_them`;
  runtime: typed "Man is smart", dbl-click edit, Home, Ctrl+Shift+Right →
  highlight band over "Man " in the canvas text (screenshot
  `word-select-man2.png`).
- Escape-keep: runtime on canvas (typed, Escape, node still had text via MCP)
  and in the layers rename (typed "ZZZnope", Escape, name unchanged). Updated
  the old discard expectation inside `text_tool_types_edits_and_removes_empty`.
- Rename field: Enter→name "PanelName" ✓; Ctrl+Enter→name `PanelNaXm\ne` ✓
  (mid-string; commit trims, so end-of-string newlines vanish by design);
  Escape→discard ✓; canvas click→commit+close ✓ (`rename-clickaway.png`).
- Root cause of the user's sticky rename field: `request_focus()` every frame
  pinned egui focus, so Enter's `lost_focus` and click-away could never fire.
- Recreated the helper, fixed SSE parsing, drove everything through MCP;
  restored grid-check to one node afterwards (nodeCount 1, "LabelTest").

## Gotchas

- The checkout has extensive **pre-existing user changes** (Linux packaging,
  `src/options.rs`, screenshots, etc.). Never revert or commit them.
- `cargo test` rewrites `docs/screenshots/`. Restore with
  `git checkout -- docs/screenshots` unless updating them on purpose.
- Don't deploy, create Vercel projects, publish releases, or push tags.
- Don't run `npm install` in the synced checkout path (Mutagen sync).
- Worktree convention in `AGENTS.md` is normally
  `../paper-rs-worktrees/<task>`, but this task intentionally edits in place.
- `.artifacts/` is gitignored; keep generated evidence there.
- egui 0.36 notes: singleline TextEdit surrenders focus on plain Enter (that is
  the commit path); egui filters typed/pasted newlines but keeps programmatic
  `\n` in the buffer; `TextEditState.cursor.char_range()` gives char indices
  for caret surgery; per-frame `request_focus` pins focus (bug pattern).
- Frame titles are fixed-size screen-space labels; editing places caret/selection
  in screen space, and clicking inside a selected frame selects children (frames
  don't capture child clicks). Group/instance roots still promote selection.

## Next steps

1. All requested changes are implemented, verified, and uncommitted on
   `agent/drawio-features`. Ask the user whether to commit/ship rather than
   assuming.
2. If shipping: commit on `agent/drawio-features`, rebase on `origin/main`,
   rerun the checks, open the PR with before/after evidence, then run
   `/greploop`. Do not deploy or push tags.
3. Skipped honestly (no backing model yet): angle/rotation, font family
   dropdown, gradient/word-wrap flags, Diagram tab, style-preset tiles.
4. Runtime screen = world + (588, 396) with the 1366x768-shaped window at 100%
   zoom; the window must be `xdotool windowsize <id> 1366 768` after every
   restart or the panel's right edge sits off the Xvfb screen.
3. Optional follow-ups: page-rename field could adopt the same
   Enter-commit/backspace-word-delete conventions made for elements (Escape and
   one-shot focus already fixed there); macOS check of Option+arrows word jump
   (arm matches alt too).

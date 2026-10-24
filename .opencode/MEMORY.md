# Sketch agent memory

Working notes for agents picking this project up in a later session. Keep this
file current when the task, environment, or verification state changes.

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

# Sketch agent memory

Working notes for agents picking this project up in a later session. Keep this
file current when the task, environment, or verification state changes.

## Current task (branch `agent/drawio-features`)

Five draw.io-style features, implemented in the existing checkout at
`/home/ibrahem/Desktop/Linux/Apps/sketch-rs` (not a separate worktree, by user
request: edits must stay inside that path).

| # | Feature | Code state | Runtime check |
|---|---|---|---|
| 1 | Image file drag-and-drop (canvas + home screen, multi-file fan-out) | done | verified |
| 2 | draw.io square grid (10 world units, major line every 50) | done | verified by pixel probe |
| 3 | Double-click text/label inside any element | done | verified (label "Hello" on rect) |
| 4 | Container tool that moves its children | done | verified (adopts + drags children) |
| 5 | Automatic white text on dark fills (WCAG luminance) | done | verified on label and sticky |

Final checks pass: `cargo fmt --check`, `git diff --check`,
`cargo clippy --all-targets -- -D warnings`, and the full `cargo test`
(118 unit + all integration suites). `docs/screenshots` was restored afterward.
Changes remain uncommitted on `agent/drawio-features`.

## Environment / how to drive the app headlessly

- Xvfb is already running: `Xvfb :99 -screen 0 1366x768x24 -nolisten tcp -ac`.
- Launch:
  `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:99 ./target/debug/sketch-rs`
- **Do not run with `RUST_LOG=trace` while testing double-click.** The trace
  spam adds enough latency between presses to exceed the 400 ms double-click
  window, so clicks look like separate single clicks. Use default logging.
- Find the window with `xdotool search --name Sketch` (title is
  `<file> — <page> — Sketch`). Window is 1440x900 but the Xvfb screen clips it
  to 1366x768.
- Menu/toolbar is at the left/top; the canvas area was
  `[[240,34] - [1180,900]]` in this run. Screen `(710,466)` mapped to editor
  `(470,432)` / world `(120,69)` at 100% zoom.
- Focus before synthetic input: `xdotool windowfocus <wid>`. Clicking canvas
  when no text edit is active makes letters run tool shortcuts; press `v` for
  Select first.
- Screenshots: `DISPLAY=:99 import -window root /tmp/opencode/artifacts/x.png`.
- MCP helper: `python3 /tmp/opencode/mcp.py <tool> '<json args>'`, endpoint
  `http://127.0.0.1:29980/mcp`. `open_file` needs `fileId`, not `path`.
  `get_basic_info`, `list_resources`, `get_node_info`, `set_text_content`,
  `create_node`/similar editing tools are available.
- Test project: `grid-check`, fileId `40e8748c29e6df44`, page
  `4407658a4075770f`, node `440765:1` (240x140 rect at world 0,0, fill
  `#202020`, now labeled "Hello").

## Implementation map

- `src/model/node.rs` — `Node.label`, `label_color`, `container`; text helpers
  (`edited_text`, `edited_text_and_style_mut`), `supports_label`,
  WCAG luminance/contrast helpers.
- `src/text.rs` — `layout_node` lays out labels; `text_origin`;
  `label_color` auto-contrast; sticky note text contrast.
- `src/render/draw.rs` — shape label draw ops.
- `src/render/scene.rs` — square grid lines and caret overlay.
- `src/editor/mod.rs` — pointer/double-click routing, `start_text_edit`,
  `end_text_edit`, `Tool::Container`, container adoption, container-aware
  frame clicks.
- `src/editor/text_edit.rs` — generalized text/label editing and styling.
- `src/editor/snap.rs` — grid snapping (10 units).
- `src/editor/shortcuts.rs` — `C` shortcut for Container.
- `src/mcp/exec.rs` — MCP `node_info` label output and `set_text_content`.
- `src/app.rs` — input forwarding, image drops.
- `src/ui/home.rs`, `src/ui/mod.rs` — home-screen image drops and toolbar.
- `tests/headless.rs`, `src/editor/tests.rs` — grid, label, contrast,
  container tests.

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

## Verification log (latest session)

- Feature 3: with default logging, select tool, double-click at screen
  `(710,466)`, type, Escape → `get_node_info` returned `label: "Hello"`.
  Earlier failures were caused by `RUST_LOG=trace`, not by the double-click
  code.
- Contrast: the `#202020` label showed pure white glyph pixels; a dark sticky
  note (`backgroundColor: "#202020"` via `update_styles` +
  `set_text_content`) also rendered white text.
- Container: created around the rectangle with `C`; the rectangle's `parentId`
  became the container id. Dragging empty container body by `(+80,+47)` moved
  the container from `(-30,-27)` to `(50,20)` and the child from `(0,0)` to
  `(80,47)`. Dashed border visible in `container-before.png`.
- Test project was restored afterwards: child moved back to root at `(0,0)`,
  label cleared, container and sticky deleted.
- Two regressions found by the full suite and fixed:
  1. `groups_follow_children_and_capture_clicks` — a group's derived bounds
     were used as snap candidates while dragging its child, adding a per-frame
     feedback offset. `snap_candidates` now skips a group parent.
  2. `m3_shapes_frames_and_text` — the Text tool treated any labelled shape as
     an existing text node (because `edited_text()` now covers labels), so it
     edited the frame instead of creating text. The Text tool now checks
     `n.kind.text().is_some()`, preserving the old behavior.
  3. That same test double-clicks a shape then drags it; add Escape in the test
     to leave label editing first. This is a test expectation update for the
     new double-click behavior, not a product workaround.

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
- Frame titles are fixed-size screen-space labels; editing places caret/selection in screen space, and clicking inside a selected frame selects children (frames don't capture child clicks). Group/instance roots still promote selection on press.

## Next steps

1. Features and checks are complete; changes are uncommitted. Ask the user
   whether to commit/ship rather than assuming.
2. If shipping: commit on `agent/drawio-features`, rebase on `origin/main`,
   rerun the checks, open the PR with before/after evidence, then run
   `/greploop`. Do not deploy or push tags.
3. Optional polish: the Text tool on a frame now creates a text node; Enter on
   a selected frame edits the frame's label (intended, but worth a sanity look).

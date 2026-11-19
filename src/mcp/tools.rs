//! Tool catalogue: names, descriptions and input schemas.
//!
//! Names and argument shapes follow Paper's MCP server where Sketch has an equivalent
//! (`analysis/paper-mcp.md` §2), so agent prompts written for Paper carry over. Sketch
//! additions: `create_shape` (whiteboard shapes and arrows), `select_nodes`, `undo`, `redo`.

use serde_json::{Value, json};

pub const INSTRUCTIONS: &str = "\
Sketch is a local design and whiteboard app with an infinite canvas. These tools read and \
edit the project open in the running app; every change is one undo step and shows up live.

- Start with get_basic_info (pages, artboards, sizes) and get_selection (what the user is \
looking at). Use list_resources / open_file / create_file to pick a project.
- For details read get_guide({ topic: \"sketch-rs-mcp-instructions\" }) once per session.
- pageId defaults to the page the user is viewing; pass it to work on another page without \
switching the user's view. Node ids are opaque strings; never show them to the user.
- Build UI with create_artboard, then write_html into it, one visual group per call (a header, \
a row, a card) so the user sees progress. Inline styles only; flexbox (display:flex, gap, \
padding, align-items, justify-content) is the layout model; no margin, grid or tables.
- Text uses Inter, or a system family from get_font_family_info. Use px for font sizes.
- Edit with update_styles, set_text_content, set_link, rename_nodes, duplicate_nodes, \
move_nodes, delete_nodes. Whiteboard shapes, sticky notes and arrows: create_shape. Hand-drawn style: \
update_styles with {\"sketch\": true, \"roughness\": 1, \"fillStyle\": \"hachure\"}.
- Check your work with get_screenshot. Artboard height is a starting point: when content \
clips set height: \"fit-content\" with update_styles.
- Call finish_working_on_nodes when done.";

fn file_id() -> Value {
    json!({"type": "string", "description": "The project to operate on (id, name or folder path from list_resources). When omitted the project open in the app is used."})
}

fn page_id() -> Value {
    json!({"type": "string", "description": "The page to operate on. When omitted the page the user is viewing is used. The user's page can change between calls; prefer explicit page ids."})
}

fn styles_schema(desc: &str) -> Value {
    json!({
        "type": "object",
        "propertyNames": {"type": "string"},
        "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}, {"type": "boolean"}]},
        "description": desc,
    })
}

fn tool(name: &str, desc: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    let mut props = props;
    if let Some(m) = props.as_object_mut()
        && !m.contains_key("fileId")
        && !matches!(
            name,
            "list_resources" | "create_file" | "get_font_family_info" | "get_guide" | "open_file"
        )
    {
        m.insert("fileId".into(), file_id());
    }
    let ann = if read_only {
        json!({"readOnlyHint": true})
    } else if name == "delete_nodes" {
        json!({"destructiveHint": true, "readOnlyHint": false})
    } else {
        json!({"destructiveHint": false, "readOnlyHint": false})
    };
    json!({
        "name": name,
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
        },
        "annotations": ann,
    })
}

/// Every tool, as MCP `Tool` JSON objects.
pub fn definitions() -> Vec<Value> {
    let node_id = |d: &str| json!({"type": "string", "description": d});
    let ids = |d: &str| json!({"type": "array", "items": {"type": "string"}, "description": d});
    vec![
        // ------------------------------------------------------------ projects and pages
        tool(
            "list_resources",
            "Lists the user's Sketch projects (local folders), most recently modified first. \
             Each item has id, name, path, page count, updatedAt, and `open` for the project open \
             in the app.",
            json!({"limit": {"type": "integer", "description": "Maximum projects to return. Defaults to 50."}}),
            &[],
            true,
        ),
        tool(
            "open_file",
            "Opens a project in the app (saving the current one) and returns a get_basic_info \
             response. Optional pageId switches the user to that page.",
            json!({
                "fileId": {"type": "string", "minLength": 1, "description": "Project id, name or folder path (see list_resources)."},
                "pageId": {"type": "string", "description": "Optional page to show."},
            }),
            &["fileId"],
            false,
        ),
        tool(
            "create_file",
            "Creates a new empty project and returns its id. Call open_file with the id to work in it.",
            json!({"name": {"type": "string", "description": "Optional project name."}}),
            &[],
            false,
        ),
        tool(
            "create_page",
            "Creates a new page at the end of the page list and returns its id, without switching \
             the user's page. Use the returned pageId in later calls.",
            json!({"name": {"type": "string", "description": "Optional page name. Defaults to \"Page N\"."}}),
            &[],
            false,
        ),
        tool(
            "rename_pages",
            "Rename one or more pages. Does not switch the user's page.",
            json!({"updates": {"type": "array", "description": "Page renames.", "items": {
                "type": "object",
                "properties": {"pageId": {"type": "string"}, "name": {"type": "string"}},
                "required": ["pageId", "name"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "list_layers",
            "The page's layers (draw.io layers): id, name, visibility, lock, and how many \
             top-level shapes are on each, bottom first. Layers are separate from groups and \
             the z-order tree.",
            json!({"pageId": page_id()}),
            &[],
            true,
        ),
        tool(
            "layer_ops",
            "Create, edit or reorganize the page's layers (draw.io layers). `ops` is a list of \
             single-op objects, applied in order: {op: add, name?}, {op: rename, layer, name}, \
             {op: delete, layer}, {op: duplicate, layer}, {op: move, layer, index}, \
             {op: show|hide|lock|unlock, layer}, {op: moveNodes, layer, nodeIds}, \
             {op: selectNodes, layer}. Buttons stay one undo step per applied op. `layer` is a \
             layer id (number, from list_layers) or the layer's name.",
            json!({
                "ops": {"type": "array", "description": "Layer operations, applied in order.",
                    "items": {"type": "object", "properties": {
                        "op": {"type": "string", "enum": ["add", "rename", "delete", "duplicate", "move", "show", "hide", "lock", "unlock", "moveNodes", "selectNodes"]},
                        "layer": {"type": "string", "description": "Layer id or name (not for add)."},
                        "name": {"type": "string", "description": "New or initial name."},
                        "index": {"type": "number", "description": "New 0-based position, bottom first (move only)."},
                        "nodeIds": {"type": "array", "items": {"type": "string"}, "description": "Nodes to move (moveNodes)."},
                    }}},
                "pageId": page_id(),
            }),
            &["ops"],
            false,
        ),
        // ------------------------------------------------------------ reading
        tool(
            "get_basic_info",
            "Essential context about the open project: project name, the page, node count, \
             artboards (top-level frames) with ids, sizes and world positions, other top-level \
             layers, fonts, and `pages` with the active one marked isActive.\n\
             - worldX/worldY are world positions; x/y are relative to the parent.\n\
             - Pass pageId to inspect another page without switching the user's view.",
            json!({"pageId": page_id()}),
            &[],
            true,
        ),
        tool(
            "get_selection",
            "The selected layers on the page the user is viewing: ids, names, types, sizes, \
             positions and the artboard each belongs to.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_node_info",
            "Details of one node: type, name, size, visibility, lock state, parent, children ids, \
             text content, x/y (relative to parent) and worldX/worldY. Errors if it doesn't exist.",
            json!({"nodeId": node_id("The node to inspect.")}),
            &["nodeId"],
            true,
        ),
        tool(
            "get_children",
            "Direct children of a node (or of the page root when nodeId is \"root\"), with ids, \
             names, types, child counts and positions.",
            json!({"nodeId": node_id("The parent node, or \"root\"."), "pageId": page_id()}),
            &["nodeId"],
            true,
        ),
        tool(
            "get_tree_summary",
            "Compact indented text tree of a subtree: type, name, id and size per node. Much \
             cheaper than get_jsx for orientation. nodeId \"root\" summarizes the whole page.",
            json!({
                "nodeId": node_id("Root of the summary, or \"root\"."),
                "depth": {"type": "number", "description": "Maximum depth (default 3, max 10)."},
                "pageId": page_id(),
            }),
            &["nodeId"],
            true,
        ),
        tool(
            "get_screenshot",
            "Render a node (or the whole page when nodeId is omitted or \"root\") to a PNG and \
             return it as an image. Scale 1 (default) is enough for layout checks; use 2 to read \
             small text. Large images are scaled down to fit API limits.",
            json!({
                "nodeId": node_id("The node to capture; omit for the whole page."),
                "scale": {"type": "number", "description": "Render scale: 1 (default) or 2."},
                "pageId": page_id(),
            }),
            &[],
            true,
        ),
        tool(
            "get_jsx",
            "JSX for a node and its descendants with inline styles (frames become flex divs, text \
             becomes spans, vectors become inline SVG). Use it to export a design to code.",
            json!({
                "nodeId": node_id("The node to convert."),
                "format": {"type": "string", "enum": ["inline-styles", "tailwind"], "description": "Only inline styles are produced; \"tailwind\" is accepted for compatibility."},
            }),
            &["nodeId"],
            true,
        ),
        tool(
            "get_computed_styles",
            "CSS styles (camelCase) of one or more nodes, as {nodeId: styles}.",
            json!({"nodeIds": ids("Nodes to read.")}),
            &["nodeIds"],
            true,
        ),
        tool(
            "get_fill_image",
            "The image used by a node's image fill, as image content.",
            json!({"nodeId": node_id("A node with an image fill.")}),
            &["nodeId"],
            true,
        ),
        tool(
            "find_nodes",
            "Find nodes by tag, text content and/or computed style. Searches the whole project \
             by default; pass pageId for one page or nodeId for a subtree. `tag` matches any node \
             tag case-insensitively with * wildcards anchored to the whole value, matching the \
             editor's find bar. `textValue` matches text the same way. `filters` are \
             {styleName, styleValue} pairs (AND), values may use * wildcards; colors match by value.",
            json!({
                "pageId": page_id(),
                "nodeId": node_id("Optional subtree root; takes priority over pageId."),
                "tag": {"type": "string"},
                "textValue": {"type": "string"},
                "filters": {"type": "array", "items": {"type": "object", "properties": {
                    "styleName": {"type": "string"}, "styleValue": {"type": "string"}}}},
            }),
            &[],
            true,
        ),
        tool(
            "get_font_family_info",
            "Whether font families are available and their weights/styles. Inter is always \
             available; system families are listed once the background font scan finishes. \
             Unavailable families render in Inter.",
            json!({"familyNames": {"type": "array", "items": {"type": "string"}}}),
            &["familyNames"],
            true,
        ),
        tool(
            "get_guide",
            "Read a guide. Topics: \"sketch-rs-mcp-instructions\" (read first), \"html\" (what \
             write_html supports), \"whiteboard\" (shapes, arrows, stickies, hand-drawn style).",
            json!({"topic": {"type": "string"}}),
            &["topic"],
            true,
        ),
        tool(
            "export",
            "Export nodes to files. `nodes` maps node ids to export settings \
             ([{format: png|svg|pdf, scale: \"2x\" | \"512w\" | \"512h\" | \"720p\"}]; [] = 2x PNG). \
             Files go to `directory` (default: the Downloads folder). Returns the paths written.",
            json!({
                "nodes": {"type": "object", "additionalProperties": {"type": "array", "items": {
                    "type": "object",
                    "properties": {
                        "format": {"type": "string", "enum": ["png", "svg", "pdf"]},
                        "scale": {"type": "string"},
                    },
                    "required": ["format"],
                }}},
                "directory": {"type": "string", "description": "Output folder (absolute path)."},
                "pageId": page_id(),
            }),
            &["nodes"],
            false,
        ),
        tool(
            "export_combined_pdf",
            "Export several nodes into one PDF, one page per node, ordered top-to-bottom then \
             left-to-right. Returns the file path.",
            json!({
                "nodeIds": ids("Nodes to put on the pages."),
                "path": {"type": "string", "description": "Output file (default: Downloads/<project>.pdf)."},
            }),
            &["nodeIds"],
            false,
        ),
        // ------------------------------------------------------------ writing
        tool(
            "create_artboard",
            "Creates an artboard (top-level frame) and returns its id; then add content with \
             write_html({mode: \"insert-children\"}). `styles` are camelCase CSS; width and height \
             are required (e.g. \"1440px\"). Artboards default to display:flex, \
             flexDirection:column and a white background, and are placed in an empty spot unless \
             left/top are given. Default sizes: desktop 1440×900, tablet 768×1024, mobile 390×844.",
            json!({
                "name": {"type": "string", "description": "Layer name."},
                "styles": {
                    "type": "object",
                    "properties": {"width": {"type": "string"}, "height": {"type": "string"}},
                    "required": ["width", "height"],
                    "additionalProperties": {"anyOf": [{"type": "string"}, {"type": "number"}]},
                    "description": "CSS styles, e.g. {\"width\": \"390px\", \"height\": \"844px\", \"backgroundColor\": \"#f5f5f5\", \"padding\": \"24px\", \"gap\": \"16px\"}",
                },
                "pageId": page_id(),
            }),
            &["name", "styles"],
            false,
        ),
        tool(
            "write_html",
            "Parse HTML with inline styles into layers. Write incrementally: each call should add \
             one visual group (a header, one list row, a card) so the user sees progress.\n\
             - mode \"insert-children\" appends the HTML inside targetNodeId (\"root\" = the page); \
             \"replace\" replaces targetNodeId with it.\n\
             - Inline styles only. Flexbox (display:flex, flex-direction, gap, padding, \
             align-items, justify-content, flex-grow, flex-wrap) is the layout model; block \
             elements stack vertically. position:absolute with left/top is supported. No margin, \
             grid or tables.\n\
             - Text: font-size, font-weight (≥600 = bold), font-style, color, line-height, \
             letter-spacing, text-align, text-decoration; <b>/<i>/<u>/<s>/<span style> make rich \
             text. font-family picks a bundled or installed family; unknown names render in \
             Inter.\n\
             - Boxes: background(-color/-image: colors, linear/radial gradients, url()), border, \
             border-radius, opacity, overflow:hidden, mix-blend-mode, width/height (px, %, \
             fit-content).\n\
             - <img src> takes paper-asset:///abs/path, file:// URLs, absolute paths or data: URIs. \
             <svg> markup is imported as vector layers.\n\
             - layer-name=\"...\" names layers; <x-sketch-clone node-id=\"ID\" style=\"...\"/> \
             clones an existing node.\n\
             Returns the ids of the created top-level layers and the styles that were ignored.",
            json!({
                "html": {"type": "string", "description": "HTML to parse into layers."},
                "targetNodeId": {"type": "string", "description": "Target node id, or \"root\" for the page."},
                "mode": {"type": "string", "enum": ["insert-children", "replace"]},
                "pageId": page_id(),
            }),
            &["html", "targetNodeId", "mode"],
            false,
        ),
        tool(
            "create_shape",
            "Create a whiteboard element: rectangle, ellipse, diamond, triangle, polygon, star, \
             line, arrow, sticky (sticky note with text) or text. Coordinates are world units, or \
             relative to parentId when given. Lines and arrows take `points` ([[x,y],...], at \
             least two) and arrows may bind to layers with startNodeId/endNodeId (the arrow then \
             follows them). `styles` takes the same CSS as update_styles plus the hand-drawn keys \
             sketch, roughness (0 architect, 1 artist, 2 cartoonist), fillStyle (hachure, \
             cross-hatch, solid, zigzag, dots), strokeStyle (solid, dashed, dotted). Arrows also \
             take route (straight, curved, elbow) and startHead/endHead (none, arrow, triangle, \
             dot, bar, diamond).",
            json!({
                "type": {"type": "string", "enum": ["rectangle", "ellipse", "diamond", "triangle", "polygon", "star", "line", "arrow", "sticky", "text"]},
                "x": {"type": "number"}, "y": {"type": "number"},
                "width": {"type": "number"}, "height": {"type": "number"},
                "points": {"type": "array", "items": {"type": "array", "items": {"type": "number"}}},
                "text": {"type": "string", "description": "Text for sticky and text."},
                "name": {"type": "string"},
                "parentId": {"type": "string", "description": "Frame to place the shape in."},
                "startNodeId": {"type": "string"}, "endNodeId": {"type": "string"},
                "route": {"type": "string", "enum": ["straight", "curved", "elbow"]},
                "startHead": {"type": "string"}, "endHead": {"type": "string"},
                "sides": {"type": "integer", "description": "Polygon sides / star points."},
                "styles": styles_schema("CSS and hand-drawn styles."),
                "pageId": page_id(),
            }),
            &["type"],
            false,
        ),
        tool(
            "search_shapes",
            "Search the native shape registry (flowchart, UML, ER, network, floor plan, org \
             chart). Matching is fuzzy over the shape name, id and keywords, best first. Returns \
             each shape's id, name, category, preferred size and keywords. Use the id (or exact \
             name) with insert_shape.",
            json!({
                "query": {"type": "string", "description": "Search text, e.g. \"cylinder\" or \"queue\". Empty lists every shape."},
                "category": {"type": "string", "description": "Optional category filter: Flowchart, UML, ER, Network, Floor plan, Org chart."},
            }),
            &[],
            true,
        ),
        tool(
            "insert_shape",
            "Insert a native registry shape (see search_shapes) as an editable node. Coordinates \
             are world units for the shape's top-left corner; width/height default to the \
             shape's preferred size. The node joins the frame under the point unless parentId \
             names a container, and takes the diagram's default style where the shape allows it.",
            json!({
                "shape": {"type": "string", "description": "Registry shape id or exact name, e.g. \"database\" or \"Queue\"."},
                "x": {"type": "number"}, "y": {"type": "number"},
                "width": {"type": "number"}, "height": {"type": "number"},
                "parentId": {"type": "string", "description": "Frame or group to place the shape in."},
                "layer": {"type": "string", "description": "Layer id or name on the page; the shape joins it."},
                "pageId": page_id(),
            }),
            &["shape"],
            false,
        ),
        tool(
            "list_templates",
            "List the bundled diagram packs and any custom template libraries in the folder \
             chosen in the app's template browser. Each template reports its id, name, \
             category, library, description, keywords and page names. Use the id (or exact \
             name) with insert_template.",
            json!({
                "query": {"type": "string", "description": "Search text over names, categories, descriptions and keywords. Empty lists everything."},
                "category": {"type": "string", "description": "Optional category filter, e.g. BPMN 2.0, C4, SysML, Kanban."},
            }),
            &[],
            true,
        ),
        tool(
            "insert_template",
            "Insert a template from list_templates as editable native content. By default one \
             template page joins the open diagram as a single undo step, centered on x/y (or \
             the viewport center). With asPages true, every template page becomes its own \
             project page. Registry shapes missing from this build are reported and skipped.",
            json!({
                "template": {"type": "string", "description": "Template id or exact name, e.g. \"bpmn-process\" or \"BPMN order process\"."},
                "page": {"type": "number", "description": "1-based template page for in-page insertion. Defaults to 1."},
                "asPages": {"type": "boolean", "description": "Add every template page as project pages instead of inserting in place."},
                "x": {"type": "number", "description": "World x of the insertion center for in-page insertion."},
                "y": {"type": "number", "description": "World y of the insertion center for in-page insertion."},
            }),
            &["template"],
            false,
        ),
        tool(
            "generate_diagram",
            "Turn Mermaid, CSV, SQL, or JSON graph text into editable native shapes and              connectors. The diagram joins the open page as one undo step, centered on x/y (or              the viewport center), or becomes its own page with asPage. Layout is deterministic              for the same input. Approximations are returned in warnings, and parse errors name              the line or token that failed.",
            json!({
                "source": {"type": "string", "enum": ["mermaid", "csv", "sql", "json"], "description": "Text format to parse."},
                "text": {"type": "string", "description": "The diagram source text."},
                "layout": {"type": "string", "enum": ["auto", "grid", "vertical", "horizontal", "vertical-tree", "horizontal-tree"], "description": "Placement for CSV and JSON. Mermaid and SQL choose their own layout."},
                "header": {"type": "boolean", "description": "CSV: treat the first row as column names and skip it as data."},
                "labelColumn": {"type": "number", "description": "CSV: 1-based column used as each shape's label."},
                "asPage": {"type": "boolean", "description": "Create a new page instead of inserting into the open one."},
                "x": {"type": "number", "description": "World x of the insertion center for in-page insertion."},
                "y": {"type": "number", "description": "World y of the insertion center for in-page insertion."},
            }),
            &["source", "text"],
            false,
        ),
        tool(
            "update_styles",
            "Update styles on one or more nodes (camelCase CSS, like React.CSSProperties). Setting \
             left/top on an artboard moves it. Hand-drawn keys: sketch, roughness, fillStyle, \
             strokeStyle, seed. Styles with no effect are returned under ignoredStyles.",
            json!({"updates": {"type": "array", "description": "Each item applies styles to nodeIds.", "items": {
                "type": "object",
                "properties": {
                    "nodeIds": {"type": "array", "items": {"type": "string"}},
                    "styles": styles_schema("CSS properties."),
                },
                "required": ["nodeIds", "styles"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "set_text_content",
            "Set the text of one or more Text or Sticky nodes (batch).",
            json!({"updates": {"type": "array", "items": {
                "type": "object",
                "properties": {"nodeId": {"type": "string"}, "textContent": {"type": "string"}},
                "required": ["nodeId", "textContent"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "set_link",
            "Set or clear custom links on nodes (batch). Page targets use page:Name or data:page/id,<id>; \
             external targets allow http, https and mailto. Unsafe schemes are refused. An empty link clears the field.",
            json!({"updates": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "nodeId": {"type": "string"},
                    "link": {"type": "string", "description": "Page target or safe external URL. Empty clears the link."},
                },
                "required": ["nodeId", "link"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "rename_nodes",
            "Rename layers (batch). Names are truncated to 50 characters.",
            json!({"updates": {"type": "array", "items": {
                "type": "object",
                "properties": {"nodeId": {"type": "string"}, "name": {"type": "string"}},
                "required": ["nodeId", "name"],
            }}}),
            &["updates"],
            false,
        ),
        tool(
            "delete_nodes",
            "Delete nodes and their descendants. Verify parents with get_node_info first when unsure.",
            json!({"nodeIds": ids("Nodes to delete.")}),
            &["nodeIds"],
            false,
        ),
        tool(
            "duplicate_nodes",
            "Deep-clone nodes. Each copy goes under parentId when given, else next to its source; \
             duplicated artboards are placed in an empty spot. Returns new ids and a \
             descendantIdMap from every original descendant id to its copy.",
            json!({"nodes": {"type": "array", "items": {
                "type": "object",
                "properties": {"id": {"type": "string"}, "parentId": {"type": "string"}},
                "required": ["id"],
            }}}),
            &["nodes"],
            false,
        ),
        tool(
            "move_nodes",
            "Move nodes keeping their ids. Each move is {nodeId, before: siblingId}, {nodeId, \
             after: siblingId} or {nodeId, parentId, index?} (parentId \"root\" = the page root). \
             Moves apply in order. In auto-layout parents this changes the visual order; in free \
             parents it changes stacking and keeps the world position.",
            json!({"moves": {"type": "array", "minItems": 1, "items": {
                "type": "object",
                "properties": {
                    "nodeId": {"type": "string"}, "before": {"type": "string"},
                    "after": {"type": "string"}, "parentId": {"type": "string"},
                    "index": {"type": "integer", "minimum": 0},
                },
                "required": ["nodeId"],
            }}}),
            &["moves"],
            false,
        ),
        tool(
            "reverse_connectors",
            "Reverse connectors: swap each connector's source and target, waypoints and fixed \
             binding points. Arrowheads stay on the line ends, so they appear at the opposite \
             end; labels keep their relative spot. One undo step. Non-connectors are reported in \
             notConnectors.",
            json!({"nodeIds": ids("Connectors to reverse.")}),
            &["nodeIds"],
            false,
        ),
        tool(
            "select_nodes",
            "Select nodes on the page the user is viewing so they can see what you mean; \
             zoom: true also frames them in the viewport. An empty list clears the selection.",
            json!({"nodeIds": ids("Nodes to select."), "zoom": {"type": "boolean"}}),
            &["nodeIds"],
            false,
        ),
        tool(
            "undo",
            "Undo the last change on a page (the user's or an agent's).",
            json!({"pageId": page_id()}),
            &[],
            false,
        ),
        tool(
            "redo",
            "Redo the last undone change on a page.",
            json!({"pageId": page_id()}),
            &[],
            false,
        ),
        tool(
            "finish_working_on_nodes",
            "Call when done creating or editing. Saves the project.",
            json!({"nodeIds": ids("Optional nodes you worked on.")}),
            &[],
            false,
        ),
    ]
}

pub const GUIDE_INSTRUCTIONS: &str = "\
# Designing in Sketch over MCP

1. Orient: get_basic_info → pages, artboards, sizes; get_selection → what the user points at.
2. Plan a brief before a new design: palette (3–5 colors), type scale (e.g. 12/14/16/20/28/40),
   spacing scale (4/8/12/16/24/32), corner radius.
3. Create an artboard sized for the device (create_artboard). Artboards are flex columns.
4. Fill it with write_html, one visual group per call, into the artboard id. Keep the returned
   ids: later calls can target them (insert-children) or replace them.
5. Iterate with update_styles / set_text_content / set_link / duplicate_nodes instead of rewriting.
6. Review with get_screenshot (scale 1; 2 for small text). Fix clipping with
   update_styles({height: \"fit-content\"}) on the artboard.
7. finish_working_on_nodes at the end (also saves the project).

Layout model: every container is an auto-layout frame (flexbox). display:flex with
flex-direction row/column, gap, padding, justify-content (start, center, end, space-between),
align-items (start, center, end; stretch is the default and makes children fill the cross axis
when the container has a definite size). flex-grow:1 fills the main axis; width:100% fills too.
height/width fit-content hug the content. position:absolute + left/top places freely.

Whiteboard: create_shape makes rectangles, ellipses, diamonds, stickies, text and arrows that
bind to shapes (they re-route when shapes move). Turn on the hand-drawn look with
{sketch: true, roughness: 1, fillStyle: \"hachure\"}.

Node ids look like \"3fa9c1:14\" (page prefix and node number). Ids stay valid across calls.";

pub const GUIDE_HTML: &str = "\
# write_html reference

Elements: div/section/header/footer/nav/main/article/aside/ul/ol/li/button/form → frames;
p/h1–h6/span/label/a/strong/em/code/pre (elements holding only text) → text layers (with a
frame around them when they have a background, border or padding); img → image rectangle;
svg → vector layers; br → line break; x-sketch-clone node-id=ID → copy of an existing layer.

Default sizes (like CSS): h1 32px bold, h2 24px bold, h3 20px bold, h4–h6 16px bold, p 16px,
small 13px, code/pre 14px; color is inherited, default black.

Styles: width, height (px, %, fit-content), left/top/right/bottom with position:absolute,
display (flex, block, none), flex-direction, flex-wrap, gap, row-gap, column-gap, padding*,
justify-content, align-items, align-self, flex-grow, overflow, background, background-color,
background-image (url(), linear-gradient(), radial-gradient()), background-size/object-fit,
border, border-color/width/style, border-radius (1–4 values), opacity, mix-blend-mode, color,
font-size, font-weight, font-style, line-height, letter-spacing, text-align, text-decoration.
Ignored (reported back): margin, grid, box-shadow, transform, filters, transitions.";

pub const GUIDE_WHITEBOARD: &str = "\
# Whiteboard elements

create_shape({type, x, y, width, height, styles}) for rectangle, ellipse, diamond, triangle,
polygon (sides), star (sides = points), sticky (text, background color), text (text).
Lines and arrows: create_shape({type: \"arrow\", points: [[x1,y1],[x2,y2]],
startNodeId, endNodeId, route: \"elbow\", endHead: \"triangle\"}). With start/end nodes the
points can be omitted: the arrow is routed between the shapes and follows them when they move.
reverse_connectors({nodeIds}) swaps an arrow's bindings and waypoints in one undo step, matching
draw.io's Reverse action. Arrowheads stay on their line ends and move to the opposite end.

Hand-drawn style (Excalidraw-like): styles {sketch: true, roughness: 0|1|2,
fillStyle: hachure|cross-hatch|solid|zigzag|dots, strokeStyle: solid|dashed|dotted, seed}.
Colors: backgroundColor (fill), borderColor/stroke, borderWidth/strokeWidth.";

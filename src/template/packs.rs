//! Built-in templates and diagram packs for M7 (ADR-010).
//!
//! Every pack is assembled from registry shapes and native node kinds. There
//! are no images, vendor icons, remote requests, or hidden code paths: the
//! result is the same kind of content a user can draw by hand, so selection,
//! styling, connectors, layers, undo and every export apply unchanged.

use super::{Template, TemplatePage};
use crate::geom::{Rect, V2, v2};
use crate::model::{
    Binding, Color, Document, Head, LineJump, Node, NodeId, NodeKind, Route, TextAlign,
};
use crate::registry;
use std::collections::HashMap;

const INK: Color = Color::INK;
const WHITE: Color = Color::WHITE;
const BLUE: Color = Color::rgb(0xda, 0xe8, 0xfc);
const BLUE_STRONG: Color = Color::rgb(0x2f, 0x6f, 0xf3);
const GREEN: Color = Color::rgb(0xd5, 0xe8, 0xd4);
const GREEN_STRONG: Color = Color::rgb(0x2d, 0x8a, 0x5b);
const YELLOW: Color = Color::rgb(0xff, 0xf2, 0xcc);
const ORANGE: Color = Color::rgb(0xff, 0xe6, 0xcc);
const RED: Color = Color::rgb(0xf8, 0xce, 0xcc);
const PURPLE: Color = Color::rgb(0xe1, 0xd5, 0xe7);
const GRAY: Color = Color::rgb(0xf2, 0xf2, 0xf2);
const MUTED: Color = Color::rgb(0x6b, 0x6b, 0x75);

/// Every shipped template. The browser shows them by category; `all` is also
/// the source of the built-in template library.
pub fn all() -> Vec<Template> {
    vec![
        flowchart(),
        uml_class(),
        er_model(),
        bpmn_process(),
        network(),
        floor_plan(),
        org_chart(),
        kanban(),
        timeline(),
        gantt(),
        c4_container(),
        sysml_requirements(),
        charts(),
        venn(),
        project_plan(),
    ]
}

fn template(
    id: &str,
    name: &str,
    category: &str,
    description: &str,
    keywords: &[&str],
    pages: Vec<TemplatePage>,
) -> Template {
    Template {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        category: category.into(),
        keywords: keywords.iter().map(|s| s.to_string()).collect(),
        thumbnail: None,
        pages,
    }
}

// ---------------------------------------------------------------------------------- builder

/// A connector endpoint: the node it binds to and the normalized fixed point.
type End = (NodeId, V2);
type Ends = (Option<End>, Option<End>);

struct Builder {
    doc: Document,
    shapes: HashMap<NodeId, String>,
}

impl Builder {
    fn new(name: &str) -> Builder {
        Builder {
            doc: Document {
                name: name.into(),
                ..Default::default()
            },
            shapes: HashMap::new(),
        }
    }

    fn add(&mut self, mut node: Node, parent: Option<NodeId>) -> NodeId {
        let id = self.doc.alloc_id();
        node.id = id;
        node.parent = parent;
        if let Some(p) = parent {
            if let Some(parent) = self.doc.nodes.get_mut(&p) {
                parent.children.push(id);
            }
        } else {
            self.doc.root.push(id);
        }
        self.doc.nodes.insert(id, node);
        id
    }

    fn shape(&mut self, shape: &str, rect: Rect, label: &str, parent: Option<NodeId>) -> NodeId {
        let def = registry::by_id(shape)
            .unwrap_or_else(|| panic!("template uses an unknown registry shape: {shape}"));
        let mut node = registry::build(def, rect, None);
        if !label.is_empty() {
            node.label = label.into();
        }
        let id = self.add(node, parent);
        self.shapes.insert(id, def.id.into());
        id
    }

    fn shape_fill(
        &mut self,
        shape: &str,
        rect: Rect,
        label: &str,
        fill: Color,
        parent: Option<NodeId>,
    ) -> NodeId {
        let id = self.shape(shape, rect, label, parent);
        if let Some(node) = self.doc.nodes.get_mut(&id) {
            node.style.set_fill(Some(fill));
        }
        id
    }

    fn rect(
        &mut self,
        rect: Rect,
        fill: Option<Color>,
        radius: f64,
        label: &str,
        parent: Option<NodeId>,
    ) -> NodeId {
        let mut node = Node::new(NodeId(0), NodeKind::Rect, rect);
        node.style.set_fill(fill);
        node.style.stroke = Some(INK);
        node.style.stroke_width = 2.0;
        node.style.corner_radius = radius;
        node.label = label.into();
        node.name = if label.is_empty() {
            "Rectangle".into()
        } else {
            first_line(label)
        };
        self.add(node, parent)
    }

    fn frame(
        &mut self,
        rect: Rect,
        name: &str,
        fill: Option<Color>,
        parent: Option<NodeId>,
    ) -> NodeId {
        let mut node = Node::new(NodeId(0), NodeKind::Frame { clip: true }, rect);
        node.name = name.into();
        node.style.set_fill(fill);
        node.style.stroke = Some(Color::rgb(0xa8, 0xa8, 0xb0));
        node.style.stroke_width = 1.5;
        self.add(node, parent)
    }

    fn text(
        &mut self,
        at: V2,
        text: &str,
        size: f64,
        color: Color,
        parent: Option<NodeId>,
    ) -> NodeId {
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Text {
                text: text.into(),
                font_size: size,
                auto_width: true,
                align: TextAlign::Left,
            },
            Rect::new(at.x, at.y, 1.0, 1.0),
        );
        node.name = first_line(text);
        node.style.set_fill(Some(color));
        self.add(node, parent)
    }

    fn line(&mut self, a: V2, b: V2, color: Color, width: f64, parent: Option<NodeId>) -> NodeId {
        let rect = Rect::from_points(a, b);
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Line {
                a: a - rect.min(),
                b: b - rect.min(),
            },
            rect,
        );
        node.style.stroke = Some(color);
        node.style.stroke_width = width;
        self.add(node, parent)
    }

    fn arrow(&mut self, a: V2, b: V2, color: Color, width: f64) -> NodeId {
        let rect = Rect::from_points(a, b);
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: vec![v2(0.0, 0.0) + (a - rect.min()), b - rect.min()],
                start: None,
                end: None,
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            rect,
        );
        node.style.stroke = Some(color);
        node.style.stroke_width = width;
        self.add(node, None)
    }

    fn anchor(&self, id: NodeId, fixed: V2) -> V2 {
        let rect = self
            .doc
            .world_rect(id)
            .unwrap_or_else(|| panic!("template connector points at a missing node"));
        v2(rect.x + rect.w * fixed.x, rect.y + rect.h * fixed.y)
    }

    fn link(
        &mut self,
        from: End,
        to: End,
        label: &str,
        route: Route,
        heads: (Head, Head),
    ) -> NodeId {
        let (from_id, from_fixed) = from;
        let (to_id, to_fixed) = to;
        let a = self.anchor(from_id, from_fixed);
        let b = self.anchor(to_id, to_fixed);
        self.link_points(
            a,
            b,
            (Some((from_id, from_fixed)), Some((to_id, to_fixed))),
            label,
            route,
            heads,
        )
    }

    fn link_points(
        &mut self,
        a: V2,
        b: V2,
        ends: Ends,
        label: &str,
        route: Route,
        heads: (Head, Head),
    ) -> NodeId {
        let (start, end) = ends;
        let rect = Rect::from_points(a, b);
        let point = |p: V2, rect: Rect| p - rect.min();
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: vec![point(a, rect), point(b, rect)],
                start: start.map(|(node, fixed)| Binding {
                    node,
                    fixed,
                    inside: false,
                    floating: false,
                }),
                end: end.map(|(node, fixed)| Binding {
                    node,
                    fixed,
                    inside: false,
                    floating: false,
                }),
                route,
                start_head: heads.0,
                end_head: heads.1,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            rect,
        );
        node.style.stroke = Some(INK);
        node.style.stroke_width = 1.6;
        node.label = label.into();
        self.add(node, None)
    }

    fn dashed(&mut self, id: NodeId, pattern: &[f64]) {
        if let Some(node) = self.doc.nodes.get_mut(&id) {
            node.style.dash = pattern.to_vec();
        }
    }

    fn into_page(mut self, name: &str) -> TemplatePage {
        // Resolve every connector once so previews and exported thumbnails show
        // the final route. Insertion re-resolves bindings against the target
        // page, so a template stays attached when its shapes move.
        let ids: Vec<NodeId> = self
            .doc
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Connector { .. }))
            .map(|n| n.id)
            .collect();
        for id in ids {
            if let Some(points) = crate::connector::resolve(&self.doc, id) {
                set_world_points(&mut self.doc, id, &points);
            }
        }
        TemplatePage::from_doc(name, &self.doc, &self.shapes)
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or("Text")
        .chars()
        .take(32)
        .collect()
}

fn local(frame: Rect, world: Rect) -> Rect {
    Rect::new(world.x - frame.x, world.y - frame.y, world.w, world.h)
}

fn local_point(frame: Rect, world: V2) -> V2 {
    world - frame.min()
}

fn set_world_points(doc: &mut Document, id: NodeId, pts: &[V2]) {
    let Some(node) = doc.nodes.get(&id) else {
        return;
    };
    let origin = doc.parent_origin(node.parent);
    let Some(bounds) = Rect::union_all(
        &pts.iter()
            .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
            .collect::<Vec<_>>(),
    ) else {
        return;
    };
    let local: Vec<V2> = pts.iter().map(|p| *p - bounds.min()).collect();
    let node = doc.nodes.get_mut(&id).unwrap();
    node.rect = bounds.translate(V2::ZERO - origin);
    if let NodeKind::Connector { points, .. } = &mut node.kind {
        *points = local;
    }
}

// ---------------------------------------------------------------------------------- packs

fn flowchart() -> Template {
    let mut b = Builder::new("Order flow");
    let start = b.shape_fill(
        "terminator",
        Rect::new(40.0, 40.0, 150.0, 50.0),
        "Start",
        GREEN,
        None,
    );
    let receive = b.shape(
        "process",
        Rect::new(40.0, 150.0, 180.0, 70.0),
        "Receive order",
        None,
    );
    let stock = b.shape(
        "decision",
        Rect::new(50.0, 280.0, 160.0, 100.0),
        "In stock?",
        None,
    );
    let ship = b.shape(
        "process",
        Rect::new(320.0, 295.0, 180.0, 70.0),
        "Ship order",
        None,
    );
    let backorder = b.shape(
        "process",
        Rect::new(50.0, 450.0, 160.0, 70.0),
        "Back order",
        None,
    );
    let end = b.shape_fill(
        "terminator",
        Rect::new(320.0, 450.0, 150.0, 50.0),
        "End",
        RED,
        None,
    );
    b.link(
        (start, v2(0.5, 1.0)),
        (receive, v2(0.5, 0.0)),
        "",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (receive, v2(0.5, 1.0)),
        (stock, v2(0.5, 0.0)),
        "",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (stock, v2(1.0, 0.5)),
        (ship, v2(0.0, 0.5)),
        "Yes",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (ship, v2(0.5, 1.0)),
        (end, v2(0.5, 0.0)),
        "",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (stock, v2(0.5, 1.0)),
        (backorder, v2(0.5, 0.0)),
        "No",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (backorder, v2(1.0, 0.5)),
        (receive, v2(0.0, 0.5)),
        "Restock",
        Route::Elbow,
        (Head::None, Head::Arrow),
    );
    let page = b.into_page("Order flow");
    template(
        "flowchart",
        "Order flow",
        "Flowchart",
        "A small order process with a decision, two branches, and a restock loop.",
        &["flowchart", "process", "decision", "start", "end"],
        vec![page],
    )
}

fn uml_class() -> Template {
    let mut b = Builder::new("UML class diagram");
    let order = b.shape_fill(
        "class",
        Rect::new(70.0, 60.0, 250.0, 170.0),
        "Order\n---\nid: UUID\nstatus: Status\n---\nsubmit()",
        BLUE,
        None,
    );
    let line = b.shape_fill(
        "class",
        Rect::new(410.0, 60.0, 230.0, 140.0),
        "OrderLine\n---\nqty: int\nprice: Money",
        YELLOW,
        None,
    );
    let customer = b.shape_fill(
        "class",
        Rect::new(70.0, 340.0, 230.0, 120.0),
        "Customer\n---\nname: String\nemail: String",
        GREEN,
        None,
    );
    let payable = b.shape_fill(
        "class",
        Rect::new(410.0, 340.0, 230.0, 120.0),
        "\u{ab}interface\u{bb}\nPayable\n---\npay()",
        PURPLE,
        None,
    );
    let note = b.shape_fill(
        "note",
        Rect::new(710.0, 160.0, 220.0, 100.0),
        "Use this class sketch as a\nstarting point. Every box\nand line stays editable.",
        GRAY,
        None,
    );
    b.link(
        (order, v2(0.5, 1.0)),
        (line, v2(0.0, 0.5)),
        "1..*",
        Route::Elbow,
        (Head::Diamond, Head::None),
    );
    b.link(
        (customer, v2(1.0, 0.5)),
        (order, v2(0.0, 0.5)),
        "1",
        Route::Elbow,
        (Head::None, Head::None),
    );
    let implements = b.link(
        (order, v2(1.0, 0.5)),
        (payable, v2(0.0, 0.5)),
        "implements",
        Route::Elbow,
        (Head::None, Head::Triangle),
    );
    b.dashed(implements, &[6.0, 4.0]);
    b.link(
        (note, v2(0.0, 0.5)),
        (line, v2(1.0, 0.5)),
        "",
        Route::Elbow,
        (Head::None, Head::None),
    );
    let page = b.into_page("Class model");
    template(
        "uml-class",
        "Class model",
        "UML",
        "Classes, an interface, an association, a composition, and a note.",
        &["uml", "class", "interface", "association", "composition"],
        vec![page],
    )
}

fn er_model() -> Template {
    let mut b = Builder::new("ER model");
    let customer = b.shape_fill(
        "entity",
        Rect::new(60.0, 80.0, 190.0, 80.0),
        "Customer",
        BLUE,
        None,
    );
    let order = b.shape_fill(
        "entity",
        Rect::new(430.0, 80.0, 190.0, 80.0),
        "Order",
        GREEN,
        None,
    );
    let places = b.shape_fill(
        "relationship",
        Rect::new(260.0, 200.0, 150.0, 90.0),
        "Places",
        YELLOW,
        None,
    );
    let customer_id = b.shape_fill(
        "key-attribute",
        Rect::new(70.0, 260.0, 160.0, 60.0),
        "customer_id",
        PURPLE,
        None,
    );
    let email = b.shape_fill(
        "attribute",
        Rect::new(290.0, 360.0, 150.0, 60.0),
        "email",
        GRAY,
        None,
    );
    let order_id = b.shape_fill(
        "key-attribute",
        Rect::new(450.0, 260.0, 150.0, 60.0),
        "order_id",
        PURPLE,
        None,
    );
    b.link(
        (customer, v2(0.5, 1.0)),
        (places, v2(0.0, 0.5)),
        "1",
        Route::Elbow,
        (Head::None, Head::None),
    );
    b.link(
        (order, v2(0.5, 1.0)),
        (places, v2(1.0, 0.5)),
        "N",
        Route::Elbow,
        (Head::None, Head::None),
    );
    b.link(
        (customer_id, v2(0.5, 0.0)),
        (customer, v2(0.5, 1.0)),
        "",
        Route::Straight,
        (Head::None, Head::None),
    );
    b.link(
        (email, v2(0.5, 0.0)),
        (customer, v2(0.8, 1.0)),
        "",
        Route::Elbow,
        (Head::None, Head::None),
    );
    b.link(
        (order_id, v2(0.5, 0.0)),
        (order, v2(0.5, 1.0)),
        "",
        Route::Straight,
        (Head::None, Head::None),
    );
    let page = b.into_page("Customers and orders");
    template(
        "er-model",
        "Entity relationship model",
        "ER",
        "Two entities, keys, attributes, a relationship, and cardinality labels.",
        &["er", "entity", "relationship", "database", "primary key"],
        vec![page],
    )
}

fn bpmn_process() -> Template {
    let mut b = Builder::new("BPMN process");
    let pool_rect = Rect::new(40.0, 40.0, 900.0, 320.0);
    let pool = b.frame(pool_rect, "Sales", Some(WHITE), None);
    let start = b.shape_fill(
        "bpmn-start",
        local(pool_rect, Rect::new(90.0, 165.0, 50.0, 50.0)),
        "",
        WHITE,
        Some(pool),
    );
    let receive = b.shape(
        "bpmn-task",
        local(pool_rect, Rect::new(190.0, 155.0, 160.0, 70.0)),
        "Receive order",
        Some(pool),
    );
    let gateway = b.shape(
        "bpmn-gateway",
        local(pool_rect, Rect::new(400.0, 160.0, 60.0, 60.0)),
        "?",
        Some(pool),
    );
    let ship = b.shape(
        "bpmn-task",
        local(pool_rect, Rect::new(520.0, 90.0, 160.0, 70.0)),
        "Ship order",
        Some(pool),
    );
    let backorder = b.shape(
        "bpmn-task",
        local(pool_rect, Rect::new(520.0, 220.0, 160.0, 70.0)),
        "Back order",
        Some(pool),
    );
    let end = b.shape_fill(
        "bpmn-end",
        local(pool_rect, Rect::new(760.0, 165.0, 50.0, 50.0)),
        "",
        WHITE,
        Some(pool),
    );
    let flow = |b: &mut Builder, a: (NodeId, V2), c: (NodeId, V2), label: &str| {
        b.link(a, c, label, Route::Straight, (Head::None, Head::Arrow))
    };
    flow(&mut b, (start, v2(1.0, 0.5)), (receive, v2(0.0, 0.5)), "");
    flow(&mut b, (receive, v2(1.0, 0.5)), (gateway, v2(0.0, 0.5)), "");
    flow(&mut b, (gateway, v2(1.0, 0.5)), (ship, v2(0.0, 0.5)), "Yes");
    flow(
        &mut b,
        (gateway, v2(0.0, 0.5)),
        (backorder, v2(0.0, 0.5)),
        "No",
    );
    flow(&mut b, (ship, v2(1.0, 0.5)), (end, v2(0.0, 0.5)), "");
    flow(&mut b, (backorder, v2(1.0, 0.5)), (end, v2(0.0, 0.5)), "");
    let page = b.into_page("Order process");
    template(
        "bpmn-process",
        "BPMN order process",
        "BPMN 2.0",
        "A pool with start and end events, tasks, a gateway, and typed flow arrows.",
        &["bpmn", "process", "event", "gateway", "pool"],
        vec![page],
    )
}

fn network() -> Template {
    let mut b = Builder::new("Network");
    let dmz = b.rect(Rect::new(260.0, 40.0, 700.0, 440.0), None, 0.0, "", None);
    b.dashed(dmz, &[8.0, 6.0]);
    if let Some(n) = b.doc.nodes.get_mut(&dmz) {
        n.name = "DMZ".into();
        n.style.stroke = Some(MUTED);
    }
    b.text(v2(280.0, 54.0), "DMZ", 15.0, MUTED, None);
    let internet = b.shape_fill(
        "cloud",
        Rect::new(60.0, 170.0, 170.0, 100.0),
        "Internet",
        BLUE,
        None,
    );
    let firewall = b.shape_fill(
        "firewall",
        Rect::new(300.0, 170.0, 150.0, 100.0),
        "Firewall",
        RED,
        None,
    );
    let web = b.shape_fill(
        "server",
        Rect::new(520.0, 120.0, 120.0, 120.0),
        "Web",
        GREEN,
        None,
    );
    let db = b.shape_fill(
        "database",
        Rect::new(740.0, 110.0, 140.0, 130.0),
        "Orders DB",
        YELLOW,
        None,
    );
    let laptop = b.shape_fill(
        "host",
        Rect::new(560.0, 320.0, 130.0, 110.0),
        "Admin",
        GRAY,
        None,
    );
    b.link(
        (internet, v2(1.0, 0.5)),
        (firewall, v2(0.0, 0.5)),
        "HTTPS",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (firewall, v2(1.0, 0.5)),
        (web, v2(0.0, 0.5)),
        "",
        Route::Elbow,
        (Head::None, Head::Arrow),
    );
    b.link(
        (web, v2(1.0, 0.5)),
        (db, v2(0.0, 0.5)),
        "SQL",
        Route::Elbow,
        (Head::None, Head::Arrow),
    );
    b.link(
        (firewall, v2(0.5, 1.0)),
        (laptop, v2(0.0, 0.5)),
        "VPN",
        Route::Elbow,
        (Head::None, Head::Arrow),
    );
    let page = b.into_page("Network");
    template(
        "network",
        "Network",
        "Network",
        "Cloud, firewall, server, database, and admin host inside a dashed DMZ.",
        &["network", "firewall", "server", "database", "cloud"],
        vec![page],
    )
}

fn floor_plan() -> Template {
    let mut b = Builder::new("Floor plan");
    let room_rect = Rect::new(40.0, 40.0, 760.0, 520.0);
    let room = b.frame(room_rect, "Apartment", Some(WHITE), None);
    let l = |r: Rect| local(room_rect, r);
    let lp = |p: V2| local_point(room_rect, p);
    b.text(lp(v2(70.0, 70.0)), "Living room", 16.0, MUTED, Some(room));
    b.text(lp(v2(430.0, 70.0)), "Bedroom", 16.0, MUTED, Some(room));
    b.text(lp(v2(430.0, 350.0)), "Kitchen", 16.0, MUTED, Some(room));
    b.line(
        lp(v2(400.0, 60.0)),
        lp(v2(400.0, 520.0)),
        MUTED,
        2.0,
        Some(room),
    );
    b.line(
        lp(v2(400.0, 320.0)),
        lp(v2(780.0, 320.0)),
        MUTED,
        2.0,
        Some(room),
    );
    b.shape(
        "window",
        l(Rect::new(80.0, 40.0, 120.0, 30.0)),
        "",
        Some(room),
    );
    b.shape(
        "window",
        l(Rect::new(500.0, 40.0, 120.0, 30.0)),
        "",
        Some(room),
    );
    b.shape(
        "door",
        l(Rect::new(390.0, 180.0, 20.0, 70.0)),
        "",
        Some(room),
    );
    b.shape(
        "door",
        l(Rect::new(610.0, 320.0, 70.0, 20.0)),
        "",
        Some(room),
    );
    b.shape(
        "stairs",
        l(Rect::new(650.0, 380.0, 120.0, 100.0)),
        "",
        Some(room),
    );
    b.rect(
        l(Rect::new(90.0, 190.0, 190.0, 80.0)),
        Some(BLUE),
        8.0,
        "Sofa",
        Some(room),
    );
    b.rect(
        l(Rect::new(120.0, 320.0, 150.0, 90.0)),
        Some(YELLOW),
        8.0,
        "Table",
        Some(room),
    );
    b.rect(
        l(Rect::new(440.0, 150.0, 190.0, 110.0)),
        Some(PURPLE),
        8.0,
        "Bed",
        Some(room),
    );
    b.rect(
        l(Rect::new(440.0, 370.0, 150.0, 60.0)),
        Some(GREEN),
        4.0,
        "Counter",
        Some(room),
    );
    let page = b.into_page("Apartment");
    template(
        "floor-plan",
        "Apartment plan",
        "Floor plan",
        "A room outline with doors, windows, stairs, and movable furniture.",
        &["floor plan", "room", "door", "window", "furniture"],
        vec![page],
    )
}

fn org_chart() -> Template {
    let mut b = Builder::new("Org chart");
    let ceo = b.rect(
        Rect::new(390.0, 50.0, 200.0, 80.0),
        Some(BLUE),
        10.0,
        "CEO\nAda Lovelace",
        None,
    );
    let engineering = b.rect(
        Rect::new(100.0, 270.0, 190.0, 80.0),
        Some(GREEN),
        10.0,
        "Engineering\nGrace Hopper",
        None,
    );
    let design = b.rect(
        Rect::new(390.0, 270.0, 190.0, 80.0),
        Some(PURPLE),
        10.0,
        "Design\nDieter Rams",
        None,
    );
    let operations = b.rect(
        Rect::new(680.0, 270.0, 190.0, 80.0),
        Some(YELLOW),
        10.0,
        "Operations\nKatherine Johnson",
        None,
    );
    let platform = b.rect(
        Rect::new(40.0, 470.0, 180.0, 70.0),
        Some(GRAY),
        10.0,
        "Platform",
        None,
    );
    let product = b.rect(
        Rect::new(240.0, 470.0, 180.0, 70.0),
        Some(GRAY),
        10.0,
        "Product",
        None,
    );
    for (from, to) in [(ceo, engineering), (ceo, design), (ceo, operations)] {
        b.link(
            (from, v2(0.5, 1.0)),
            (to, v2(0.5, 0.0)),
            "",
            Route::Elbow,
            (Head::None, Head::None),
        );
    }
    for (from, to) in [(engineering, platform), (engineering, product)] {
        b.link(
            (from, v2(0.5, 1.0)),
            (to, v2(0.5, 0.0)),
            "",
            Route::Elbow,
            (Head::None, Head::None),
        );
    }
    b.shape("person", Rect::new(760.0, 60.0, 70.0, 90.0), "", None);
    b.text(
        v2(730.0, 160.0),
        "Reporting lines follow\nthe boxes you move.",
        14.0,
        MUTED,
        None,
    );
    let page = b.into_page("Org chart");
    template(
        "org-chart",
        "Org chart",
        "Org chart",
        "A manager and teams with reporting lines that stay attached when boxes move.",
        &["org chart", "team", "people", "reporting", "tree"],
        vec![page],
    )
}

fn kanban_page() -> TemplatePage {
    let mut b = Builder::new("Kanban board");
    let board_rect = Rect::new(40.0, 40.0, 1000.0, 560.0);
    let board = b.frame(board_rect, "Sprint board", Some(WHITE), None);
    let board_local = |r: Rect| local(board_rect, r);
    let board_point = |p: V2| local_point(board_rect, p);
    b.text(
        board_point(v2(24.0, 20.0)),
        "Sprint 42",
        24.0,
        INK,
        Some(board),
    );
    b.text(
        board_point(v2(780.0, 28.0)),
        "Week 2 of 2",
        14.0,
        MUTED,
        Some(board),
    );
    let columns = [
        ("Backlog", BLUE, 20.0),
        ("In progress", YELLOW, 340.0),
        ("Done", GREEN, 660.0),
    ];
    let mut cols = Vec::new();
    for (name, color, x) in columns {
        let col_rect = board_local(Rect::new(x, 80.0, 300.0, 450.0));
        let col = b.frame(col_rect, name, Some(color), Some(board));
        b.text(v2(16.0, 32.0), "0 cards", 13.0, MUTED, Some(col));
        cols.push((col, col_rect));
    }
    // Cards in the first column.
    let (first_col, first_rect) = cols[0];
    let (second_col, second_rect) = cols[1];
    let (third_col, third_rect) = cols[2];
    for (i, (title, color)) in [("Import from Figma", BLUE), ("Export presets", PURPLE)]
        .into_iter()
        .enumerate()
    {
        b.rect(
            local(
                first_rect,
                Rect::new(
                    first_rect.x + 16.0,
                    first_rect.y + 50.0 + i as f64 * 92.0,
                    268.0,
                    76.0,
                ),
            ),
            Some(color),
            8.0,
            title,
            Some(first_col),
        );
    }
    for (i, (title, color)) in [("Connector labels", YELLOW), ("Layer ordering", ORANGE)]
        .into_iter()
        .enumerate()
    {
        b.rect(
            local(
                second_rect,
                Rect::new(
                    second_rect.x + 16.0,
                    second_rect.y + 50.0 + i as f64 * 92.0,
                    268.0,
                    76.0,
                ),
            ),
            Some(color),
            8.0,
            title,
            Some(second_col),
        );
    }
    b.rect(
        local(
            third_rect,
            Rect::new(third_rect.x + 16.0, third_rect.y + 50.0, 268.0, 76.0),
        ),
        Some(GREEN),
        8.0,
        "Template packs",
        Some(third_col),
    );
    b.into_page("Sprint board")
}

fn kanban() -> Template {
    template(
        "kanban-board",
        "Kanban board",
        "Kanban",
        "Three columns with draggable cards for backlog, work in progress, and done.",
        &["kanban", "board", "cards", "sprint", "planning"],
        vec![kanban_page()],
    )
}

fn timeline_page() -> TemplatePage {
    let mut b = Builder::new("Timeline");
    b.text(v2(60.0, 40.0), "Product roadmap", 26.0, INK, None);
    b.arrow(v2(70.0, 320.0), v2(940.0, 320.0), INK, 2.0);
    for (x, phase, color) in [
        (110.0, "Research", BLUE),
        (350.0, "Build", GREEN),
        (590.0, "Beta", YELLOW),
        (810.0, "Launch", ORANGE),
    ] {
        b.rect(
            Rect::new(x, 120.0, 180.0, 60.0),
            Some(color),
            8.0,
            phase,
            None,
        );
        b.line(v2(x + 90.0, 180.0), v2(x + 90.0, 300.0), MUTED, 1.5, None);
        b.shape_fill(
            "venn-circle",
            Rect::new(x + 60.0, 290.0, 60.0, 60.0),
            "",
            color,
            None,
        );
    }
    for (x, label) in [
        (80.0, "Jan"),
        (320.0, "Mar"),
        (560.0, "May"),
        (780.0, "Jul"),
    ] {
        b.text(v2(x, 360.0), label, 14.0, MUTED, None);
    }
    b.text(
        v2(60.0, 430.0),
        "Move a phase or milestone and its marker stays on the line.",
        14.0,
        MUTED,
        None,
    );
    b.into_page("Roadmap")
}

fn timeline() -> Template {
    template(
        "timeline",
        "Product roadmap",
        "Timeline",
        "Phases with milestones on a dated line.",
        &["timeline", "roadmap", "milestone", "phase", "schedule"],
        vec![timeline_page()],
    )
}

fn gantt_page() -> TemplatePage {
    let mut b = Builder::new("Gantt");
    b.text(v2(60.0, 40.0), "Launch schedule", 26.0, INK, None);
    let left = 230.0;
    let top = 120.0;
    let width = 620.0;
    for (i, day) in ["Mon", "Tue", "Wed", "Thu", "Fri"].iter().enumerate() {
        b.text(
            v2(left + i as f64 * width / 4.0 + 6.0, top - 32.0),
            day,
            13.0,
            MUTED,
            None,
        );
        b.line(
            v2(left + i as f64 * width / 4.0, top),
            v2(left + i as f64 * width / 4.0, 430.0),
            GRAY,
            1.0,
            None,
        );
    }
    b.line(v2(left, top), v2(left + width, top), MUTED, 1.5, None);
    let tasks = [
        ("Design", 0.0, 1.4, BLUE),
        ("Build", 1.0, 2.6, GREEN),
        ("Test", 2.2, 1.5, YELLOW),
        ("Launch", 4.0, 0.8, ORANGE),
    ];
    for (i, (name, start, span, color)) in tasks.into_iter().enumerate() {
        let y = top + 42.0 + i as f64 * 58.0;
        b.text(v2(60.0, y + 6.0), name, 16.0, INK, None);
        b.rect(
            Rect::new(left + start * width / 4.0, y, span * width / 4.0, 34.0),
            Some(color),
            6.0,
            "",
            None,
        );
        b.line(
            v2(left, y + 17.0),
            v2(left + width, y + 17.0),
            GRAY,
            1.0,
            None,
        );
    }
    b.into_page("Schedule")
}

fn gantt() -> Template {
    template(
        "gantt",
        "Launch schedule",
        "Gantt",
        "Task rows, week columns, and resizable bars.",
        &["gantt", "schedule", "bar", "tasks", "weeks"],
        vec![gantt_page()],
    )
}

fn c4_container() -> Template {
    let mut b = Builder::new("C4 container");
    let customer = b.shape_fill(
        "c4-person",
        Rect::new(50.0, 230.0, 90.0, 130.0),
        "Customer",
        GRAY,
        None,
    );
    b.text(v2(30.0, 370.0), "Customer", 15.0, INK, None);
    let boundary_rect = Rect::new(220.0, 60.0, 610.0, 440.0);
    let boundary = b.frame(boundary_rect, "Banking system", None, None);
    b.dashed(boundary, &[8.0, 6.0]);
    let web = b.shape_fill(
        "c4-container",
        local(boundary_rect, Rect::new(270.0, 150.0, 190.0, 100.0)),
        "Web app\n[Container: Svelte]",
        BLUE,
        Some(boundary),
    );
    let api = b.shape_fill(
        "c4-container",
        local(boundary_rect, Rect::new(540.0, 150.0, 190.0, 100.0)),
        "API\n[Container: Rust]",
        GREEN,
        Some(boundary),
    );
    let db = b.shape_fill(
        "database",
        local(boundary_rect, Rect::new(540.0, 340.0, 190.0, 110.0)),
        "Database\n[Container: PostgreSQL]",
        YELLOW,
        Some(boundary),
    );
    let email = b.shape_fill(
        "c4-system",
        Rect::new(890.0, 230.0, 190.0, 110.0),
        "Email system\n[Software system]",
        PURPLE,
        None,
    );
    b.link(
        (customer, v2(1.0, 0.5)),
        (web, v2(0.0, 0.5)),
        "Uses",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (web, v2(1.0, 0.5)),
        (api, v2(0.0, 0.5)),
        "Calls",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (api, v2(0.5, 1.0)),
        (db, v2(0.5, 0.0)),
        "Reads and writes",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (api, v2(1.0, 0.5)),
        (email, v2(0.0, 0.5)),
        "Sends",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    let page = b.into_page("Container diagram");
    template(
        "c4-container",
        "C4 container diagram",
        "C4",
        "A person, a system boundary, containers, a database, and labelled relationships.",
        &["c4", "container", "system", "boundary", "architecture"],
        vec![page],
    )
}

fn sysml_requirements() -> Template {
    let mut b = Builder::new("SysML requirements");
    let vehicle = b.shape_fill(
        "sysml-block",
        Rect::new(80.0, 80.0, 210.0, 110.0),
        "\u{ab}block\u{bb}\nVehicle",
        BLUE,
        None,
    );
    let powertrain = b.shape_fill(
        "sysml-block",
        Rect::new(390.0, 80.0, 210.0, 110.0),
        "\u{ab}block\u{bb}\nPowertrain",
        GREEN,
        None,
    );
    let speed = b.shape_fill(
        "sysml-requirement",
        Rect::new(390.0, 300.0, 220.0, 100.0),
        "\u{ab}requirement\u{bb}\nMax speed\n< 200 km/h",
        YELLOW,
        None,
    );
    let range = b.shape_fill(
        "sysml-requirement",
        Rect::new(80.0, 300.0, 220.0, 100.0),
        "\u{ab}requirement\u{bb}\nFuel range\n> 600 km",
        ORANGE,
        None,
    );
    b.shape_fill(
        "sysml-port",
        Rect::new(280.0, 110.0, 26.0, 26.0),
        "",
        WHITE,
        None,
    );
    b.link(
        (vehicle, v2(1.0, 0.5)),
        (powertrain, v2(0.0, 0.5)),
        "contains",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (powertrain, v2(0.5, 1.0)),
        (speed, v2(0.5, 0.0)),
        "satisfies",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.link(
        (vehicle, v2(0.5, 1.0)),
        (range, v2(0.5, 0.0)),
        "satisfies",
        Route::Straight,
        (Head::None, Head::Arrow),
    );
    b.text(
        v2(700.0, 90.0),
        "A block, a port, and two\nrequirements with trace links.",
        14.0,
        MUTED,
        None,
    );
    let page = b.into_page("Vehicle requirements");
    template(
        "sysml-requirements",
        "SysML requirements",
        "SysML",
        "Blocks, a port, requirements, and satisfy links.",
        &["sysml", "block", "requirement", "port", "trace"],
        vec![page],
    )
}

fn charts() -> Template {
    let mut b = Builder::new("Charts");
    b.text(v2(60.0, 30.0), "Quarterly report", 26.0, INK, None);
    // Bar chart.
    b.line(v2(90.0, 110.0), v2(90.0, 360.0), INK, 1.5, None);
    b.line(v2(90.0, 360.0), v2(390.0, 360.0), INK, 1.5, None);
    for (i, (label, value, color)) in [
        ("Q1", 120.0, BLUE_STRONG),
        ("Q2", 180.0, GREEN_STRONG),
        ("Q3", 90.0, Color::rgb(0xd9, 0x9b, 0x1f)),
        ("Q4", 210.0, Color::rgb(0xc0, 0x5a, 0x4a)),
    ]
    .into_iter()
    .enumerate()
    {
        let x = 120.0 + i as f64 * 64.0;
        b.rect(
            Rect::new(x, 360.0 - value, 38.0, value),
            Some(color),
            2.0,
            "",
            None,
        );
        b.text(v2(x + 6.0, 370.0), label, 13.0, MUTED, None);
    }
    b.text(v2(90.0, 400.0), "Revenue by quarter", 14.0, MUTED, None);
    // Line chart.
    b.line(v2(470.0, 110.0), v2(470.0, 360.0), INK, 1.5, None);
    b.line(v2(470.0, 360.0), v2(750.0, 360.0), INK, 1.5, None);
    let pts = [
        v2(500.0, 300.0),
        v2(570.0, 250.0),
        v2(640.0, 280.0),
        v2(710.0, 150.0),
    ];
    for pair in pts.windows(2) {
        b.line(pair[0], pair[1], GREEN_STRONG, 3.0, None);
    }
    for (i, p) in pts.iter().enumerate() {
        b.shape_fill(
            "venn-circle",
            Rect::new(p.x - 8.0, p.y - 8.0, 16.0, 16.0),
            "",
            GREEN_STRONG,
            None,
        );
        b.text(
            v2(p.x - 8.0, 370.0),
            format!("W{}", i + 1).as_str(),
            13.0,
            MUTED,
            None,
        );
    }
    b.text(v2(470.0, 400.0), "Weekly active users", 14.0, MUTED, None);
    // Pie chart.
    for (i, color) in [
        BLUE_STRONG,
        GREEN_STRONG,
        Color::rgb(0xd9, 0x9b, 0x1f),
        Color::rgb(0xc0, 0x5a, 0x4a),
    ]
    .into_iter()
    .enumerate()
    {
        let id = b.shape_fill(
            "pie-slice",
            Rect::new(820.0, 140.0, 180.0, 180.0),
            "",
            color,
            None,
        );
        if let Some(n) = b.doc.nodes.get_mut(&id) {
            n.style.rotation = i as f64 * 90.0;
        }
    }
    b.text(v2(820.0, 400.0), "Share by channel", 14.0, MUTED, None);
    let page = b.into_page("Charts");
    template(
        "charts",
        "Chart board",
        "Charts",
        "A bar chart, a line chart, and a pie chart made from editable native shapes.",
        &["chart", "bar", "line", "pie", "graph"],
        vec![page],
    )
}

fn venn() -> Template {
    let mut b = Builder::new("Venn");
    b.text(v2(60.0, 30.0), "Audience overlap", 26.0, INK, None);
    let a = b.shape_fill(
        "venn-circle",
        Rect::new(170.0, 110.0, 300.0, 300.0),
        "",
        Color::rgba(0x2f, 0x6f, 0xf3, 0x80),
        None,
    );
    let c = b.shape_fill(
        "venn-circle",
        Rect::new(390.0, 110.0, 300.0, 300.0),
        "",
        Color::rgba(0x2d, 0x8a, 0x5b, 0x80),
        None,
    );
    let e = b.shape_fill(
        "venn-circle",
        Rect::new(280.0, 260.0, 300.0, 300.0),
        "",
        Color::rgba(0xd9, 0x9b, 0x1f, 0x80),
        None,
    );
    for id in [a, c, e] {
        if let Some(n) = b.doc.nodes.get_mut(&id) {
            n.style.stroke_width = 2.0;
        }
    }
    b.text(v2(120.0, 80.0), "Design", 18.0, BLUE_STRONG, None);
    b.text(v2(640.0, 80.0), "Engineering", 18.0, GREEN_STRONG, None);
    b.text(
        v2(380.0, 600.0),
        "Product",
        18.0,
        Color::rgb(0x9b, 0x6b, 0x0f),
        None,
    );
    b.text(v2(370.0, 310.0), "Shared\nroadmap", 18.0, INK, None);
    b.text(
        v2(60.0, 660.0),
        "Change a circle's fill opacity to keep overlaps readable.",
        14.0,
        MUTED,
        None,
    );
    let page = b.into_page("Venn");
    template(
        "venn",
        "Venn diagram",
        "Venn",
        "Three overlapping circles with translucent fills and labels.",
        &["venn", "overlap", "set", "circles", "audience"],
        vec![page],
    )
}

fn project_plan() -> Template {
    template(
        "project-plan",
        "Project plan",
        "Project planning",
        "Three pages that start a sprint board, a roadmap, and a launch schedule.",
        &["project", "planning", "kanban", "timeline", "gantt"],
        vec![kanban_page(), timeline_page(), gantt_page()],
    )
}

use sketch_rs::app::Start;
use sketch_rs::editor::Editor;
use sketch_rs::io::export::Appearance;
use sketch_rs::io::{embed, export, file, import, png, svg};
use sketch_rs::model::{Color, ColorMode, Document};
use sketch_rs::project::{MANIFEST, Store};
use sketch_rs::render::{frame, gpu::Headless};
use sketch_rs::session::Session;
use sketch_rs::ui::UiState;
use sketch_rs::ui::home::HomeView;
use std::borrow::Cow;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Sketch: infinite canvas for planning, working and sketching

USAGE:
  sketch-rs                                 open the projects home screen
  sketch-rs PROJECT_DIR                     open a project folder
  sketch-rs FILE.sketch-rs                  open a Sketch 0.1 document (not autosaved)
  sketch-rs --demo                          open the app with a sample board
  sketch-rs --screenshot OUT.png [--home | PROJECT|FILE|--demo] [--size WxH] [--scale S]
                                            render the full UI headlessly to a PNG
  sketch-rs --export-png OUT.png PROJECT|FILE [--page N] [--scale S] [--transparent] [--embed-data] [--appearance light|dark]
  sketch-rs --export-svg OUT.svg PROJECT|FILE [--page N] [--embed-data] [--appearance light|dark]
  sketch-rs --export-pdf OUT.pdf PROJECT|FILE [--page N] [--frames | --all-pages] [--appearance light|dark]
  sketch-rs --export-drawio OUT.drawio PROJECT|FILE
                                            every page in one draw.io file
  sketch-rs --export OUT PROJECT|FILE --format png|svg|pdf|drawio [--scope page|frames|all]
                                            [--page N] [--scale S] [--transparent] [--embed-data]
                                            [--appearance light|dark]
                                            (frames/all with PNG/SVG write a folder)
  sketch-rs --import FILE                   create a project from .excalidraw, .svg,
                                            .drawio, .xml or .sketch-rs and print its folder
  sketch-rs --help

Projects live in ~/Documents/sketch-rs (override with SKETCH_RS_HOME).
";

fn is_project(p: &std::path::Path) -> bool {
    p.join(MANIFEST).is_file()
}

fn load_editor(path: Option<&str>, demo: bool) -> Result<Editor, String> {
    if demo {
        return Ok(sketch_rs::demo::showcase());
    }
    let Some(p) = path else {
        return Ok(Editor::default());
    };
    let p = PathBuf::from(p);
    if is_project(&p) {
        let mut ed = Editor::default();
        let store = Store::open(p.parent().unwrap_or(&p)).map_err(|e| e.to_string())?;
        Session::new(store)
            .open(&mut ed, &p)
            .map_err(|e| format!("{}: {e}", p.display()))?;
        return Ok(ed);
    }
    let (doc, cam) = file::load(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut ed = Editor::default();
    ed.load(doc, cam, Some(p));
    Ok(ed)
}

fn baked_doc<'a>(
    doc: &'a Document,
    appearance: Appearance,
) -> (Cow<'a, Document>, ColorMode, Option<Color>) {
    let mode = ColorMode::new(doc.settings.adaptive_colors, appearance.is_dark());
    let draw_doc = if mode.is_dark() {
        Cow::Owned(doc.with_appearance(mode))
    } else {
        Cow::Borrowed(doc)
    };
    let authored_bg = if doc.settings.background_enabled {
        doc.settings.background_color
    } else {
        Color::WHITE
    };
    let bg = Some(mode.resolve(authored_bg, doc.settings.background_color_dark));
    (draw_doc, mode, bg)
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let positional: Vec<&String> = {
        let mut out = Vec::new();
        let mut skip = false;
        for a in &args {
            if skip {
                skip = false;
                continue;
            }
            if matches!(
                a.as_str(),
                "--screenshot"
                    | "--export-png"
                    | "--export-svg"
                    | "--export-pdf"
                    | "--export-drawio"
                    | "--export"
                    | "--appearance"
                    | "--format"
                    | "--scope"
                    | "--import"
                    | "--size"
                    | "--scale"
                    | "--page"
            ) {
                skip = true;
                continue;
            }
            if !a.starts_with("--") {
                out.push(a);
            }
        }
        out
    };
    let input = positional.first().map(|s| s.as_str());
    let scale: f64 = value("--scale")
        .map(|s| s.parse().map_err(|_| "bad --scale"))
        .transpose()?
        .unwrap_or(2.0);
    let appearance = match value("--appearance").as_deref().unwrap_or("light") {
        "light" => Appearance::Light,
        "dark" => Appearance::Dark,
        other => return Err(format!("unknown --appearance {other}")),
    };

    if flag("--help") || flag("-h") {
        print!("{USAGE}");
        return Ok(());
    }
    let page: usize = value("--page")
        .map(|s| s.parse().map_err(|_| "bad --page"))
        .transpose()?
        .unwrap_or(0);
    let load_pages = |input: Option<&str>| -> Result<Vec<sketch_rs::project::PageData>, String> {
        if flag("--demo") {
            Ok(vec![sketch_rs::project::PageData {
                id: "demo".into(),
                name: "Launch plan".into(),
                doc: sketch_rs::demo::showcase().doc,
                camera: None,
            }])
        } else {
            import::load_any(input.ok_or("missing input file")?.as_ref())
        }
    };
    let embed_json = |pages: &[sketch_rs::project::PageData], page: usize| {
        flag("--embed-data").then(|| {
            let name = if flag("--demo") {
                "Launch plan".to_string()
            } else {
                input
                    .and_then(|p| {
                        PathBuf::from(p)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| "Sketch".into())
            };
            sketch_rs::project::format::pages_to_json(
                &name,
                pages,
                pages.get(page).map(|p| p.id.as_str()),
            )
        })
    };
    if let Some(out) = value("--screenshot") {
        let mut ed = load_editor(input, flag("--demo"))?;
        let mut st = UiState::default();
        if flag("--home") {
            let store = Store::open(Store::default_root()).map_err(|e| e.to_string())?;
            st.home = Some(HomeView::load(&store, sketch_rs::project::now_ms()));
        }
        let (w, h) = match value("--size") {
            Some(s) => {
                let (w, h) = s.split_once('x').ok_or("--size must be WxH")?;
                (
                    w.parse::<f32>().map_err(|_| "bad width")?,
                    h.parse::<f32>().map_err(|_| "bad height")?,
                )
            }
            None => (1440.0, 900.0),
        };
        let gpu = Headless::new()?;
        let img = frame::capture(&gpu, &mut ed, &mut st, [w, h], scale as f32, |ed| {
            ed.zoom_to_fit()
        });
        img.save(&out).map_err(|e| e.to_string())?;
        println!(
            "wrote {out} ({}x{}) via {}",
            img.width(),
            img.height(),
            gpu.adapter_info.name
        );
        return Ok(());
    }
    if let Some(out) = value("--export-png") {
        let pages = load_pages(input)?;
        let doc = &pages
            .get(page)
            .ok_or_else(|| format!("no page {page}"))?
            .doc;
        let (_, mode, baked_bg) = baked_doc(doc, appearance);
        let bg = (!flag("--transparent")).then_some(baked_bg.unwrap_or(Color::WHITE));
        let data = embed_json(&pages, page);
        let (w, h) =
            png::export_with_data_mode(out.as_ref(), doc, None, scale, bg, data.as_deref(), mode)?;
        println!("wrote {out} ({w}x{h})");
        return Ok(());
    }
    if let Some(out) = value("--export-svg") {
        let pages = load_pages(input)?;
        let doc = &pages
            .get(page)
            .ok_or_else(|| format!("no page {page}"))?
            .doc;
        let (draw_doc, _, baked_bg) = baked_doc(doc, appearance);
        let mut s = svg::export(draw_doc.as_ref(), None, baked_bg).ok_or("nothing to export")?;
        if let Some(json) = embed_json(&pages, page) {
            s = embed::svg_with_data(&s, &json);
        }
        std::fs::write(&out, s).map_err(|e| e.to_string())?;
        println!("wrote {out}");
        return Ok(());
    }
    let export_with =
        |out: &str, format: export::Format, scope: export::Scope| -> Result<(), String> {
            let pages = load_pages(input)?;
            if page >= pages.len() {
                return Err(format!("no page {page}"));
            }
            let refs: Vec<(&str, &sketch_rs::model::Document)> =
                pages.iter().map(|p| (p.name.as_str(), &p.doc)).collect();
            let spec = export::Spec {
                format,
                scope,
                scale,
                transparent: flag("--transparent"),
                appearance,
                embed_data: flag("--embed-data"),
            };
            let items = export::items(&spec, &refs, page, &[]);
            let data = embed_json(&pages, page);
            let files =
                export::write_with_data(&spec, &items, out.as_ref(), None, data.as_deref())?;
            for f in files {
                println!("wrote {}", f.display());
            }
            Ok(())
        };
    if let Some(out) = value("--export-pdf") {
        let scope = if flag("--all-pages") {
            export::Scope::AllPages
        } else if flag("--frames") {
            export::Scope::Frames
        } else {
            export::Scope::Page
        };
        return export_with(&out, export::Format::Pdf, scope);
    }
    if let Some(out) = value("--export-drawio") {
        return export_with(&out, export::Format::Drawio, export::Scope::AllPages);
    }
    if let Some(out) = value("--export") {
        let format = match value("--format").as_deref().unwrap_or("png") {
            "png" => export::Format::Png,
            "svg" => export::Format::Svg,
            "pdf" => export::Format::Pdf,
            "drawio" => export::Format::Drawio,
            f => return Err(format!("unknown --format {f}")),
        };
        if format == export::Format::Drawio {
            return export_with(&out, format, export::Scope::AllPages);
        }
        let scope = match value("--scope").as_deref().unwrap_or("page") {
            "page" => export::Scope::Page,
            "frames" => export::Scope::Frames,
            "all" => export::Scope::AllPages,
            s => return Err(format!("unknown --scope {s}")),
        };
        return export_with(&out, format, scope);
    }
    if let Some(src) = value("--import") {
        let p = PathBuf::from(&src);
        let store = Store::open(Store::default_root()).map_err(|e| e.to_string())?;
        let name = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Imported".into());
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mut report = None;
        if let Some((bundle_name, pages, _)) = import::embedded_pages(&p)? {
            let bundle_name = if bundle_name.trim().is_empty() {
                name
            } else {
                bundle_name
            };
            let proj = store
                .create_with_pages(&bundle_name, pages)
                .map_err(|e| e.to_string())?;
            for pg in proj.load_pages().map_err(|e| e.to_string())? {
                sketch_rs::assets::save_to_dir(
                    &proj.dir.join(sketch_rs::project::ASSETS_DIR),
                    sketch_rs::assets::used_by(&pg.doc),
                )
                .map_err(|e| e.to_string())?;
            }
            println!("{}", proj.dir.display());
            return Ok(());
        }
        let pages = match ext.as_str() {
            "drawio" | "xml" => {
                let t = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
                let (pages, r) = sketch_rs::io::drawio::import_str(&t)?;
                report = Some(r);
                pages
            }
            "excalidraw" | "json" => {
                let t = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
                vec![sketch_rs::project::PageData {
                    id: sketch_rs::project::new_id(),
                    name: "Page 1".into(),
                    doc: sketch_rs::io::excalidraw::import_scene(&t)?,
                    camera: None,
                }]
            }
            "svg" => {
                let t = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
                vec![sketch_rs::project::PageData {
                    id: sketch_rs::project::new_id(),
                    name: "Page 1".into(),
                    doc: sketch_rs::io::svg_import::import_page(&t, &name)?,
                    camera: None,
                }]
            }
            _ => import::import_as_pages(&p)?.1,
        };
        let proj = store
            .create_with_pages(&name, pages)
            .map_err(|e| e.to_string())?;
        for pg in proj.load_pages().map_err(|e| e.to_string())? {
            sketch_rs::assets::save_to_dir(
                &proj.dir.join(sketch_rs::project::ASSETS_DIR),
                sketch_rs::assets::used_by(&pg.doc),
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some(r) = report {
            println!("{}", r.summary());
            for d in r.approximated.iter().chain(&r.dropped) {
                println!("  {} {} {}: {}", d.page, d.cell, d.item, d.reason);
            }
            for w in &r.warnings {
                println!("  warning: {w}");
            }
        }
        println!("{}", proj.dir.display());
        return Ok(());
    }
    let start = match input {
        _ if flag("--demo") => Start::Scratch(Box::new(load_editor(None, true)?)),
        None => Start::Home,
        Some(p) if is_project(p.as_ref()) => Start::Project(PathBuf::from(p)),
        Some(p) => Start::Scratch(Box::new(load_editor(Some(p), false)?)),
    };
    sketch_rs::app::run(start)
}

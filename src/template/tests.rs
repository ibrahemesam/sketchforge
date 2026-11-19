use super::*;
use crate::geom::v2;

fn every_template() -> Vec<Template> {
    packs::all()
}

#[test]
fn bundled_library_round_trips_through_json() {
    let lib = TemplateLibrary::bundled();
    lib.validate().unwrap();
    let json = lib.to_json();
    let back = TemplateLibrary::from_json(&json).unwrap();
    assert_eq!(back.name, lib.name);
    assert_eq!(back.templates, lib.templates);
}

#[test]
fn every_pack_page_builds_a_valid_document() {
    for t in every_template() {
        assert!(
            !t.description.trim().is_empty(),
            "{} has no description",
            t.id
        );
        assert!(!t.keywords.is_empty(), "{} has no keywords", t.id);
        assert!(!t.pages.is_empty(), "{} has no pages", t.id);
        for (i, page) in t.pages.iter().enumerate() {
            let (page, missing) = page.sanitize();
            assert!(missing.is_empty(), "{} page {i} lost {missing:?}", t.id);
            let doc = page.to_doc();
            doc.validate()
                .unwrap_or_else(|e| panic!("{} page {i}: {e}", t.id));
            assert!(!doc.root.is_empty(), "{} page {i} is empty", t.id);
            assert!(page.bounds().is_some(), "{} page {i} has no bounds", t.id);
        }
    }
}

#[test]
fn packs_cover_every_documented_category() {
    let categories: HashSet<String> = every_template()
        .into_iter()
        .map(|t| t.category.to_lowercase())
        .collect();
    for wanted in [
        "flowchart",
        "uml",
        "er",
        "bpmn 2.0",
        "network",
        "floor plan",
        "org chart",
        "kanban",
        "timeline",
        "gantt",
        "c4",
        "sysml",
        "charts",
        "venn",
    ] {
        assert!(categories.contains(wanted), "no pack in {wanted}");
    }
}

#[test]
fn connectors_keep_their_bindings_after_sanitizing() {
    let mut checked = 0;
    for t in every_template() {
        for page in &t.pages {
            let (clean, missing) = page.sanitize();
            assert!(missing.is_empty());
            for item in &clean.nodes {
                if let NodeKind::Connector { start, end, .. } = &item.node.kind {
                    for binding in [start, end].into_iter().flatten() {
                        checked += 1;
                        assert!(
                            clean.nodes.iter().any(|n| n.node.id == binding.node),
                            "{} points at a missing node",
                            t.id
                        );
                    }
                }
            }
        }
    }
    assert!(checked > 0, "no pack has a bound connector");
}

#[test]
fn a_missing_registry_shape_is_reported_and_its_links_are_cleared() {
    let mut t = every_template()
        .into_iter()
        .find(|t| t.id == "flowchart")
        .unwrap();
    let page = &mut t.pages[0];
    let victim = page
        .nodes
        .iter()
        .find(|n| n.shape.as_deref() == Some("terminator"))
        .map(|n| n.node.id)
        .unwrap();
    let before = page.nodes.len();
    for item in &mut page.nodes {
        if item.node.id == victim {
            item.shape = Some("does-not-exist".into());
        }
    }
    let (clean, missing) = page.sanitize();
    assert_eq!(missing, vec!["does-not-exist".to_string()]);
    assert_eq!(clean.nodes.len(), before - 1);
    assert!(!clean.root.contains(&victim));
    for item in &clean.nodes {
        if let NodeKind::Connector { start, end, .. } = &item.node.kind {
            assert_ne!(start.as_ref().map(|b| b.node), Some(victim));
            assert_ne!(end.as_ref().map(|b| b.node), Some(victim));
        }
        assert!(!item.node.children.contains(&victim));
    }
}

#[test]
fn a_custom_folder_loads_valid_libraries_and_reports_bad_files() {
    let dir = std::env::temp_dir().join(format!("sketch-templates-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    let mut t = every_template().remove(0);
    t.fill_thumbnail();
    let lib = TemplateLibrary {
        name: "Mine".into(),
        templates: vec![t],
        ..TemplateLibrary::default()
    };
    lib.save(&dir.join("mine.json")).unwrap();
    std::fs::write(dir.join("notes.txt"), b"ignore me").unwrap();
    std::fs::write(dir.join("broken.json"), b"{\"format\":\"nope\"}").unwrap();

    let (libs, errors) = TemplateLibrary::load_folder(&dir);
    assert_eq!(libs.len(), 1);
    assert_eq!(libs[0].name, "Mine");
    assert!(
        libs[0]
            .path
            .as_ref()
            .is_some_and(|p| p.ends_with("mine.json"))
    );
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("broken.json"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn save_template_writes_a_reloadable_file_with_a_thumbnail() {
    let dir = std::env::temp_dir().join(format!("sketch-template-{:016x}", fastrand::u64(..)));
    let t = every_template().remove(0);
    let path = TemplateLibrary::save_template(&t, &dir).unwrap();
    assert!(path.to_string_lossy().ends_with(TEMPLATE_SUFFIX));
    let lib = TemplateLibrary::load_file(&path).unwrap();
    assert_eq!(lib.templates.len(), 1);
    assert_eq!(lib.templates[0].name, t.name);
    assert!(
        lib.templates[0]
            .thumbnail
            .as_ref()
            .is_some_and(|s| s.contains("<svg"))
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn page_data_names_pages_and_gives_them_fresh_ids() {
    let t = every_template()
        .into_iter()
        .find(|t| t.id == "project-plan")
        .unwrap();
    let (pages, missing) = t.page_data();
    assert!(missing.is_empty());
    assert_eq!(pages.len(), 3);
    let ids: HashSet<String> = pages.iter().map(|p| p.id.clone()).collect();
    assert_eq!(ids.len(), pages.len());
    for (i, page) in pages.iter().enumerate() {
        assert!(page.name.starts_with(&t.name), "{}", page.name);
        assert!(page.name.contains(&t.pages[i].name));
        page.doc.validate().unwrap();
    }
}

#[test]
fn template_pages_map_layers_by_name_when_inserting() {
    let t = every_template().remove(0);
    let page = &t.pages[0];
    assert!(!page.nodes.is_empty());
    let background = Layer::background();
    let mut layers = HashMap::new();
    layers.insert(background.id, None);
    let (item, missing) = t.insert_item(0, &layers).unwrap();
    assert!(missing.is_empty());
    for node in &item.nodes {
        assert_eq!(node.layer, None);
    }
    assert!(item.bounds().is_some());
    let center = item.bounds().unwrap().center();
    assert!(center.x.is_finite() && center.y.is_finite());
    let _ = v2(0.0, 0.0);
}

#[test]
fn a_wrong_format_or_newer_version_is_refused() {
    let mut other = TemplateLibrary::bundled();
    other.format = "other".into();
    let e = TemplateLibrary::from_json(&other.to_json()).unwrap_err();
    assert!(e.contains("not a Sketch template library"));
    let mut lib = TemplateLibrary::bundled();
    lib.version = TEMPLATE_VERSION + 1;
    let json = serde_json::to_string(&lib).unwrap();
    let e = TemplateLibrary::from_json(&json).unwrap_err();
    assert!(e.contains("newer than this app supports"));
}

#[test]
fn validation_rejects_duplicate_ids_and_dangling_roots() {
    let mut lib = TemplateLibrary::bundled();
    let mut dup = lib.templates[0].clone();
    dup.name = "Copy".into();
    lib.templates.push(dup);
    assert!(
        lib.validate()
            .unwrap_err()
            .contains("duplicate template id")
    );

    let mut lib = TemplateLibrary::bundled();
    lib.templates[0].pages[0].root.push(NodeId(999_999));
    assert!(lib.validate().unwrap_err().contains("missing root"));
}

#[test]
fn search_matches_names_categories_and_keywords() {
    let t = every_template()
        .into_iter()
        .find(|t| t.id == "bpmn-process")
        .unwrap();
    assert!(t.matches("bpmn"));
    assert!(t.matches("gateway"));
    assert!(t.matches("BPMN order"));
    assert!(!t.matches("venn"));
}

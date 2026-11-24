//! Safe subset of draw.io's custom-link action JSON.
//!
//! draw.io stores interactive links as `data:action/json,<JSON>`. This module
//! accepts only visibility and selection actions. A link cannot ask the native
//! app to run code or open an unapproved target.

use serde_json::Value;

/// Prefix draw.io uses for custom action links.
pub const PREFIX: &str = "data:action/json,";

/// One supported action from a custom link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Toggle(Targets),
    Show(Targets),
    Hide(Targets),
    Select(Targets),
}

/// The cells, layers and tags an action applies to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Targets {
    pub cells: Vec<String>,
    pub layers: Vec<String>,
    pub tags: Vec<String>,
    /// draw.io treats an empty `tags` selector as every tagged cell.
    pub all_tags: bool,
    pub exclude_cells: Vec<String>,
    pub tags_match: TagsMatch,
}

impl Targets {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() && self.layers.is_empty() && self.tags.is_empty() && !self.all_tags
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TagsMatch {
    #[default]
    And,
    Or,
}

/// Parse the JSON half of a custom action link. The parser refuses unsupported
/// effects. A link should not claim to do more than the app can do.
pub fn parse(json: &str) -> Result<Vec<Action>, String> {
    let value: Value =
        serde_json::from_str(json.trim()).map_err(|e| format!("action JSON is invalid: {e}"))?;
    let steps = value
        .get("actions")
        .and_then(Value::as_array)
        .ok_or_else(|| "action link has no action list".to_string())?;
    if steps.is_empty() {
        return Err("action link has no actions".into());
    }

    let mut out = Vec::with_capacity(steps.len());
    for step in steps {
        let obj = step
            .as_object()
            .ok_or_else(|| "an action step is not an object".to_string())?;
        for key in ["toggle", "show", "hide", "select"] {
            let Some(target) = obj.get(key) else { continue };
            let targets =
                targets(target).map_err(|e| format!("{key} action has invalid targets: {e}"))?;
            if targets.is_empty() {
                return Err(format!("{key} action has no targets"));
            }
            let action = match key {
                "toggle" => Action::Toggle(targets),
                "show" => Action::Show(targets),
                "hide" => Action::Hide(targets),
                _ => Action::Select(targets),
            };
            out.push(action);
        }
        // Timing metadata is safe to ignore. An unsupported effect is not.
        // Skipping it would make the link misleading, even when the same step
        // also contains a supported action.
        let unsupported = obj
            .keys()
            .find(|k| {
                !matches!(
                    k.as_str(),
                    "toggle" | "show" | "hide" | "select" | "wait" | "immediate" | "delay"
                )
            })
            .cloned();
        if let Some(key) = unsupported {
            return Err(format!("unsupported action “{key}”"));
        }
    }
    Ok(out)
}

fn targets(value: &Value) -> Result<Targets, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "targets are not an object".to_string())?;
    let tags = strings(obj.get("tags"))?;
    let all_tags = obj
        .get("tags")
        .is_some_and(|value| value.as_array().is_some_and(|items| items.is_empty()));
    Ok(Targets {
        cells: strings(obj.get("cells"))?,
        layers: strings(obj.get("layers"))?,
        tags,
        all_tags,
        exclude_cells: strings(obj.get("excludeCells"))?,
        tags_match: match obj.get("tagsMatch").and_then(Value::as_str) {
            Some("or") | Some("OR") => TagsMatch::Or,
            _ => TagsMatch::And,
        },
    })
}

fn strings(value: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let arr = value
        .as_array()
        .ok_or_else(|| "expected an array of strings".to_string())?;
    arr.iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| "expected an array of strings".to_string())
        })
        .collect()
}

/// Rewrite cell and layer ids in an action link. Import and export use this to
/// translate between draw.io and native ids. Tags and unknown fields stay
/// unchanged.
pub fn remap_ids(
    link: &str,
    mut map_cell: impl FnMut(&str) -> String,
    mut map_layer: impl FnMut(&str) -> String,
) -> String {
    let Some(json) = link.strip_prefix(PREFIX) else {
        return link.to_string();
    };
    let Ok(mut value) = serde_json::from_str::<Value>(json.trim()) else {
        return link.to_string();
    };
    let Some(steps) = value.get_mut("actions").and_then(Value::as_array_mut) else {
        return link.to_string();
    };
    for step in steps.iter_mut() {
        let Some(obj) = step.as_object_mut() else {
            continue;
        };
        for target in obj.values_mut() {
            let Some(fields) = target.as_object_mut() else {
                continue;
            };
            for (key, field) in fields.iter_mut() {
                let Some(items) = field.as_array_mut() else {
                    continue;
                };
                for item in items.iter_mut() {
                    let Some(id) = item.as_str() else { continue };
                    let replacement = match key.as_str() {
                        "cells" | "excludeCells" => map_cell(id),
                        "layers" => map_layer(id),
                        _ => id.to_string(),
                    };
                    *item = Value::String(replacement);
                }
            }
        }
    }
    match serde_json::to_string(&value) {
        Ok(json) => format!("{PREFIX}{json}"),
        Err(_) => link.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_drawio_toggle_show_hide_and_select() {
        let link = r#"{"actions":[{"toggle":{"layers":["2"]}},{"show":{"cells":["n3"]}},{"hide":{"tags":["old"]}},{"select":{"cells":["*"]}}]}"#;
        let actions = parse(link).unwrap();
        assert_eq!(actions.len(), 4);
        assert!(matches!(&actions[0], Action::Toggle(t) if t.layers == ["2"]));
        assert!(matches!(&actions[1], Action::Show(t) if t.cells == ["n3"]));
        assert!(matches!(&actions[2], Action::Hide(t) if t.tags == ["old"]));
        assert!(matches!(&actions[3], Action::Select(t) if t.cells == ["*"]));
    }

    #[test]
    fn refuses_unsupported_effects_and_empty_lists() {
        assert!(parse(r#"{"actions":[{"highlight":{"cells":["1"]}}]}"#).is_err());
        assert!(
            parse(r#"{"actions":[{"show":{"cells":["1"]},"highlight":{"cells":["2"]}}]}"#).is_err()
        );
        assert!(parse(r#"{"actions":[{"toggle":{"cells":[]}}]}"#).is_err());
        assert!(parse(r#"{"actions":[]}"#).is_err());
        assert!(parse(r#"{"actions":[{"toggle":"2"}]}"#).is_err());
    }

    #[test]
    fn parses_empty_tags_as_all_tagged_cells() {
        let actions = parse(r#"{"actions":[{"show":{"tags":[]}}]}"#).unwrap();
        assert!(matches!(&actions[0], Action::Show(t) if t.tags.is_empty()));
    }

    #[test]
    fn ignores_wait_metadata_beside_a_supported_action() {
        let actions =
            parse(r#"{"actions":[{"wait":50},{"show":{"cells":["1"]},"immediate":true}]}"#)
                .unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Show(_)));
    }

    #[test]
    fn remaps_only_id_fields() {
        let link = "data:action/json,{\"actions\":[{\"toggle\":{\"cells\":[\"3\"],\"layers\":[\"2\"],\"excludeCells\":[\"4\"],\"tags\":[\"keep\"]}}]}";
        let out = remap_ids(link, |id| format!("n{id}"), |id| format!("l{id}"));
        let json = out.strip_prefix(PREFIX).unwrap();
        let v: Value = serde_json::from_str(json).unwrap();
        assert_eq!(v["actions"][0]["toggle"]["cells"][0], "n3");
        assert_eq!(v["actions"][0]["toggle"]["layers"][0], "l2");
        assert_eq!(v["actions"][0]["toggle"]["excludeCells"][0], "n4");
        assert_eq!(v["actions"][0]["toggle"]["tags"][0], "keep");
    }
}

//! TypeScript declarations from a tool's JSON Schema, for `orbyn_code_describe`.

use serde_json::Value;

/// `declare function name(args: {...}): Promise<unknown>;` with doc comments from descriptions.
pub(super) fn declaration(spec: &Value) -> String {
    let name = spec["name"].as_str().unwrap_or("unknown");
    let mut out = String::new();
    if let Some(description) = spec["description"].as_str() {
        out.push_str(&doc(description, ""));
    }
    let args = type_of(&spec["inputSchema"], 0);
    out.push_str(&format!(
        "declare function {name}(args: {args}): Promise<unknown>;"
    ));
    out
}

fn doc(text: &str, indent: &str) -> String {
    let text = text.trim().replace("*/", "* /");
    if text.is_empty() {
        return String::new();
    }
    format!("{indent}/** {} */\n", text.replace('\n', " "))
}

fn type_of(schema: &Value, depth: usize) -> String {
    if let Some(options) = schema["enum"].as_array() {
        let all: Vec<String> = options.iter().map(Value::to_string).collect();
        if !all.is_empty() {
            return all.join(" | ");
        }
    }
    match schema["type"].as_str() {
        Some("string") => "string".into(),
        Some("integer" | "number") => "number".into(),
        Some("boolean") => "boolean".into(),
        Some("null") => "null".into(),
        Some("array") => {
            let item = schema
                .get("items")
                .map_or_else(|| "unknown".to_owned(), |i| type_of(i, depth));
            if item.contains(" | ") {
                format!("({item})[]")
            } else {
                format!("{item}[]")
            }
        }
        Some("object") => object(schema, depth),
        _ => "unknown".into(),
    }
}

fn object(schema: &Value, depth: usize) -> String {
    let Some(properties) = schema["properties"].as_object().filter(|p| !p.is_empty()) else {
        return "Record<string, unknown>".into();
    };
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let pad = "  ".repeat(depth + 1);
    let mut out = String::from("{\n");
    for (key, value) in properties {
        if let Some(description) = value["description"].as_str() {
            out.push_str(&doc(description, &pad));
        }
        let optional = if required.contains(&key.as_str()) {
            ""
        } else {
            "?"
        };
        out.push_str(&format!(
            "{pad}{key}{optional}: {};\n",
            type_of(value, depth + 1)
        ));
    }
    out.push_str(&format!("{}}}", "  ".repeat(depth)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flat_arguments_become_an_interface_with_optional_marks() {
        let spec = json!({
            "name": "orbyn_thread_send",
            "description": "Send a message.",
            "inputSchema": { "type": "object",
                "properties": { "thread_id": { "type": "string", "description": "The thread." }, "n": { "type": "integer" } },
                "required": ["thread_id"] },
        });
        let text = declaration(&spec);
        assert!(
            text.starts_with("/** Send a message. */\ndeclare function orbyn_thread_send(args: {"),
            "{text}"
        );
        assert!(
            text.contains("  /** The thread. */\n  thread_id: string;"),
            "{text}"
        );
        assert!(text.contains("  n?: number;"), "{text}");
        assert!(text.ends_with("}): Promise<unknown>;"), "{text}");
    }

    #[test]
    fn enums_arrays_and_nested_objects_are_typed() {
        let schema = json!({ "type": "object", "properties": {
            "mode": { "enum": ["a", "b"] },
            "tags": { "type": "array", "items": { "type": "string" } },
            "either": { "type": "array", "items": { "enum": [1, 2] } },
            "inner": { "type": "object", "properties": { "x": { "type": "boolean" } }, "required": ["x"] },
        }});
        let text = type_of(&schema, 0);
        assert!(text.contains("mode?: \"a\" | \"b\";"), "{text}");
        assert!(text.contains("tags?: string[];"), "{text}");
        assert!(text.contains("either?: (1 | 2)[];"), "{text}");
        assert!(text.contains("inner?: {\n    x: boolean;\n  };"), "{text}");
    }

    #[test]
    fn a_description_cannot_close_the_comment() {
        let spec =
            json!({ "name": "t", "description": "a */ b\nc", "inputSchema": { "type": "object" } });
        let text = declaration(&spec);
        assert!(text.starts_with("/** a * / b c */\n"), "{text}");
        assert!(text.contains("args: Record<string, unknown>"), "{text}");
    }

    #[test]
    fn every_orbyn_tool_has_a_declaration_matching_its_required_arguments() {
        for spec in crate::tools::mcp_tools::tool_specs(true) {
            let text = declaration(&spec);
            let name = spec["name"].as_str().unwrap();
            assert!(
                text.contains(&format!("declare function {name}(")),
                "{name}"
            );
            for required in spec["inputSchema"]["required"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let key = required.as_str().unwrap();
                assert!(
                    text.contains(&format!("{key}: ")),
                    "{name}: {key} is required"
                );
            }
        }
    }
}

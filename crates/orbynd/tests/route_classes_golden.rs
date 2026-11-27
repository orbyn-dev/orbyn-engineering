//! Golden pin for the protected route classes (#844).
//!
//! `tests/routes.json` is generated from `src/routes/classes.rs`, so a class and the code that
//! decides it are one edit apart: regenerating the table after a code change accepts a silent
//! reclassification, and moving a `Person` route to `Local` keeps every other route-table test
//! green. This test compares the whole `(method, path) -> class` map in `tests/routes.json` with the
//! explicit table committed in `tests/fixtures/route_classes_golden.json`. The fixture is data, never
//! written at test time, and a new, dropped or reclassified route fails here with the route and both
//! classes.
//!
//! To change a class on purpose (and only then):
//! 1. edit the route in `src/routes/classes.rs` (`PERSON` with its reason, and `ENFORCED` for the
//!    key), the web client's list in `web/src/api/personKey.ts`, and `tests/routes.json` by
//!    regenerating it: `ORBYN_UPDATE_ROUTES=1 cargo test -p orbynd --bin orbynd route_table`;
//! 2. edit the same row in `tests/fixtures/route_classes_golden.json` to the new class, keeping the
//!    one-row-per-route order so the change is one line in the diff;
//! 3. say in the commit and the pull request why the route changed class.
//!
//! The second edit is the review gate: a reclassification shows up as a line in the fixture's diff.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

/// Reads one JSON file from the crate's `tests/` tree.
fn read(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

/// The `(method, path) -> class` map of the protected rows of a route table.
fn classes(path: &Path) -> BTreeMap<(String, String), String> {
    let table = read(path);
    let rows = table["protected"]
        .as_array()
        .unwrap_or_else(|| panic!("{} has no protected array", path.display()));
    let mut classes = BTreeMap::new();
    for row in rows {
        let (Some(method), Some(route), Some(class)) = (
            row.get(0).and_then(Value::as_str),
            row.get(1).and_then(Value::as_str),
            row.get(2).and_then(Value::as_str),
        ) else {
            panic!(
                "{}: a protected row is not [method, path, class]: {row}",
                path.display()
            );
        };
        assert!(
            matches!(class, "local" | "person"),
            "{}: {method} {route} has class {class:?}, not local or person",
            path.display()
        );
        assert!(
            classes
                .insert((method.to_owned(), route.to_owned()), class.to_owned())
                .is_none(),
            "{}: {method} {route} is listed twice",
            path.display()
        );
    }
    assert!(!classes.is_empty(), "{} pins no routes", path.display());
    classes
}

/// Every protected route keeps the class the fixture records. A reclassification, an addition and a
/// removal all fail here; a class change names the route and both the expected and the current class.
#[test]
fn the_protected_route_classes_match_the_golden_fixture() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let current = classes(&manifest.join("tests/routes.json"));
    let expected = classes(&manifest.join("tests/fixtures/route_classes_golden.json"));
    let mut changes = Vec::new();
    for ((method, route), class) in &expected {
        match current.get(&(method.clone(), route.clone())) {
            None => changes.push(format!(
                "{method} {route}: expected class {class}, found no protected route"
            )),
            Some(found) if found != class => changes.push(format!(
                "{method} {route}: expected class {class}, found class {found}"
            )),
            Some(_) => {}
        }
    }
    for ((method, route), class) in &current {
        if !expected.contains_key(&(method.clone(), route.clone())) {
            changes.push(format!(
                "{method} {route}: expected no protected route, found class {class}"
            ));
        }
    }
    assert!(
        changes.is_empty(),
        concat!(
            "the protected route classes differ from tests/fixtures/route_classes_golden.json:\n",
            "  {}\n",
            "If a class really changed, edit the fixture in the same diff and say why; ",
            "never regenerate it.",
        ),
        changes.join("\n  ")
    );
}

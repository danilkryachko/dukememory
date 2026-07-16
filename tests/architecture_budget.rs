use std::fs;
use std::path::Path;

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn direct_dependency_surface_cannot_expand_without_review() {
    let manifest = fs::read_to_string(project_root().join("Cargo.toml")).unwrap();
    let value = manifest.parse::<toml::Value>().unwrap();
    let dependencies = value
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .expect("[dependencies] table");
    let target_dependencies = value
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(|target| target.get("dependencies"))
        .filter_map(toml::Value::as_table)
        .map(toml::map::Map::len)
        .sum::<usize>();
    let optional = dependencies
        .values()
        .filter(|dependency| {
            dependency
                .as_table()
                .and_then(|table| table.get("optional"))
                .and_then(toml::Value::as_bool)
                == Some(true)
        })
        .count();

    assert!(
        dependencies.len() + target_dependencies <= 23,
        "direct runtime dependencies grew from the reviewed budget of 23"
    );
    assert!(
        optional <= 5,
        "optional runtime dependencies grew from the reviewed budget of 5"
    );
}

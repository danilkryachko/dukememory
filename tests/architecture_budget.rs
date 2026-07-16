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
        dependencies.len() <= 22,
        "direct runtime dependencies grew from the reviewed budget of 22"
    );
    assert!(
        optional <= 5,
        "optional runtime dependencies grew from the reviewed budget of 5"
    );
}

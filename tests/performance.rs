use assert_cmd::Command;
use rusqlite::{Connection, params};
use serde_json::Value;
use tempfile::tempdir;

fn command(db: &std::path::Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("dukememory"));
    command.arg("--db").arg(db);
    command
}

#[test]
#[ignore = "runs in the dedicated CI performance gate"]
fn vector_search_stays_within_reviewed_ci_thresholds() {
    const VECTOR_COUNT: usize = 4_096;
    const DIMENSIONS: usize = 96;

    let directory = tempdir().unwrap();
    let db = directory.path().join("performance.db");
    command(&db).arg("list").arg("--json").assert().success();

    let mut connection = Connection::open(&db).unwrap();
    let transaction = connection.transaction().unwrap();
    for index in 0..VECTOR_COUNT {
        let memory_id = format!("perf-{index:05}");
        transaction
            .execute(
                r#"
                INSERT INTO memories(
                    id, type, scope, title, body, status, created_at, updated_at, confidence
                ) VALUES (?1, 'note', 'project', ?2, ?3, 'active', ?4, ?4, 1.0)
                "#,
                params![
                    memory_id,
                    format!("Performance fixture {index}"),
                    format!("Deterministic vector benchmark fixture {index}"),
                    index as i64 + 1,
                ],
            )
            .unwrap();
        let mut embedding = vec![0.0_f32; DIMENSIONS];
        embedding[index % DIMENSIONS] = 1.0;
        embedding[(index * 7 + 3) % DIMENSIONS] = 0.5;
        transaction
            .execute(
                r#"
                INSERT INTO memory_embeddings(
                    memory_id, model, endpoint, dimensions, embedding, content_hash, updated_at
                ) VALUES (?1, 'performance-fixture', 'mock:local', ?2, ?3, ?4, ?5)
                "#,
                params![
                    memory_id,
                    DIMENSIONS as i64,
                    serde_json::to_string(&embedding).unwrap(),
                    format!("hash-{index}"),
                    index as i64 + 1,
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();

    let output = command(&db)
        .arg("vector-bench")
        .arg("--provider")
        .arg("mock")
        .arg("--endpoint")
        .arg("local")
        .arg("--model")
        .arg("performance-fixture")
        .arg("--iterations")
        .arg("25")
        .arg("--warmup")
        .arg("5")
        .arg("--limit")
        .arg(VECTOR_COUNT.to_string())
        .arg("--max-p95-ms")
        .arg("750")
        .arg("--min-qps")
        .arg("1")
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();

    assert_eq!(report["version"], 4);
    assert_eq!(report["vectors"], VECTOR_COUNT);
    assert_eq!(report["dimensions"], DIMENSIONS);
    assert_eq!(report["thresholds"]["ok"], true);
    assert!(report["thresholds"]["observed_p95_ms"].as_f64().unwrap() > 0.0);
    assert!(report["thresholds"]["observed_qps"].as_f64().unwrap() > 0.0);
}

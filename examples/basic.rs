//! Basic Deltex Rust SDK example — demonstrates CRUD and transactions.
//!
//! Run with:
//!   export DELTEX_API_KEY="dtx_k_..."
//!   cargo run --example basic --features async

use deltex::{Client, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let db = Client::from_env()?;

    // ── Schema setup ──────────────────────────────────────────────────────────
    db.execute_raw("CREATE TABLE IF NOT EXISTS rust_demo (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        score FLOAT,
        active BOOLEAN DEFAULT TRUE
    )").await?;
    // The engine rejects un-scoped DELETE (safety guard) — scope it explicitly.
    db.execute_raw("DELETE FROM rust_demo WHERE id >= 0").await?;

    // ── INSERT via parameterized query ────────────────────────────────────────
    let alice_id = 1i64;
    let bob_id   = 2i64;
    db.execute(
        "INSERT INTO rust_demo (id, name, score, active) VALUES ($1, $2, $3, $4)",
        &[&alice_id, &"Alice", &9.5f64, &true],
    ).await?;
    db.execute(
        "INSERT INTO rust_demo (id, name, score, active) VALUES ($1, $2, $3, $4)",
        &[&bob_id, &"Bob", &7.2f64, &false],
    ).await?;
    println!("✓ Inserted Alice and Bob");

    // ── SELECT multiple rows ──────────────────────────────────────────────────
    let rows = db.query("SELECT * FROM rust_demo ORDER BY id", &[]).await?;
    for row in &rows {
        println!(
            "  id={} name={} score={} active={}",
            row.get("id").unwrap_or(&deltex::Value::Null),
            row.get("name").unwrap_or(&deltex::Value::Null),
            row.get("score").unwrap_or(&deltex::Value::Null),
            row.get("active").unwrap_or(&deltex::Value::Null),
        );
    }

    // ── SELECT one row ────────────────────────────────────────────────────────
    let alice = db.query_one(
        "SELECT * FROM rust_demo WHERE id = $1",
        &[&alice_id],
    ).await?;
    println!("✓ Found Alice: name={}", alice["name"]);

    // ── UPDATE ────────────────────────────────────────────────────────────────
    let updated = db.execute(
        "UPDATE rust_demo SET score = $1 WHERE id = $2",
        &[&10.0f64, &alice_id],
    ).await?;
    println!("✓ Updated {} row(s)", updated);

    // ── Aggregation ───────────────────────────────────────────────────────────
    let agg = db.query_one("SELECT AVG(score) AS avg_score, COUNT(*) AS n FROM rust_demo", &[]).await?;
    println!(
        "✓ Aggregate: avg_score={} n={}",
        agg["avg_score"], agg["n"]
    );

    // ── Transaction via builder ───────────────────────────────────────────────
    let mut txn = db.begin();
    txn.add("INSERT INTO rust_demo (id, name, score) VALUES ($1, $2, $3)", &[&3i64, &"Carol", &8.8f64]);
    txn.add("UPDATE rust_demo SET active = $1 WHERE id = $2", &[&true, &bob_id]);
    db.commit(txn).await?;
    println!("✓ Transaction committed (Carol inserted, Bob activated)");

    // ── Verify final state ────────────────────────────────────────────────────
    let final_rows = db.query("SELECT name, score FROM rust_demo ORDER BY id", &[]).await?;
    println!("Final state:");
    for row in &final_rows {
        println!("  {} → {}", row["name"], row["score"]);
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    db.execute_raw("DROP TABLE IF EXISTS rust_demo").await?;
    println!("✓ Cleanup done");

    Ok(())
}

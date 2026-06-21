# Deltex Rust SDK

Official Rust client for [Deltex](https://deltex.dev) — edge-native SQL database.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
deltex = "1.3"
```

## Quick Start

```rust
use deltex::{Client, Options};

#[tokio::main]
async fn main() -> deltex::Result<()> {
    // Connect using DELTEX_API_KEY env var
    let db = Client::from_env()?;

    // Query
    let rows = db.query("SELECT * FROM users WHERE active = $1", &[&true]).await?;

    // Insert
    db.execute(
        "INSERT INTO events (type, user_id) VALUES ($1, $2)",
        &[&"click", &42i64],
    ).await?;

    // Transaction
    let mut txn = db.begin();
    txn.add("UPDATE balances SET amount = amount - $1 WHERE id = $2", &[&100i64, &1i64]);
    txn.add("UPDATE balances SET amount = amount + $1 WHERE id = $2", &[&100i64, &2i64]);
    db.commit(txn).await?;

    Ok(())
}
```

## API

### Client creation

```rust
// From environment variables (DELTEX_API_KEY, DELTEX_URL)
let db = Client::from_env()?;

// Explicit options
let db = Client::connect(Options::from_env()?
    .with_api_key("dtx_k_...")
    .with_url("https://db.deltex.dev")
    .with_write_mode("sync")
    .with_timeout(30)
)?;
```

### Queries

```rust
// Multiple rows
let rows: Vec<Row> = db.query("SELECT * FROM users", &[]).await?;

// Single row (errors if none)
let row: Row = db.query_one("SELECT * FROM users WHERE id = $1", &[&1i64]).await?;

// Optional row
let row: Option<Row> = db.query_opt("SELECT * FROM users WHERE id = $1", &[&999i64]).await?;

// Execute (returns affected row count)
let n: usize = db.execute("DELETE FROM sessions WHERE expired = $1", &[&true]).await?;

// Raw SQL
let rows = db.execute_raw("SHOW TABLES").await?;
```

### Parameter Binding

Parameters use positional `$1`, `$2`, etc. placeholders. Supported types:

| Rust type | SQL equivalent |
|-----------|---------------|
| `i32`, `i64`, `u32`, `u64` | INTEGER |
| `f32`, `f64` | FLOAT |
| `bool` | BOOLEAN |
| `&str`, `String` | TEXT (auto-escaped) |
| `Option<T>` | NULL or the inner type |

```rust
// String parameters are automatically single-quote escaped
db.execute(
    "INSERT INTO notes (title, content) VALUES ($1, $2)",
    &[&"O'Brien's note", &"It's working!"],
).await?;
```

### Transactions

```rust
// Builder pattern (recommended — explicit control)
let mut txn = db.begin();
txn.add("INSERT INTO orders (item) VALUES ($1)", &[&"widget"]);
txn.add("UPDATE inventory SET count = count - 1 WHERE item = $1", &[&"widget"]);
db.commit(txn).await?;

// Raw statements
let mut txn = db.begin();
txn.add_raw("UPDATE stats SET requests = requests + 1");
db.commit(txn).await?;
```

### Read Consistency

```rust
// Strong consistency — reads bypass cache, always hits KV
let strong_db = db.strong();
let row = strong_db.query_one("SELECT * FROM critical_data WHERE id = $1", &[&id]).await?;

// Custom write mode
let edge_db = db.with_write_mode("edge");
```

### Blocking (Sync) Client

```rust
use deltex::blocking::Client;

let db = Client::from_env()?;
let rows = db.query("SELECT * FROM users", &[])?;
```

Enable with:
```toml
deltex = { version = "1.3", features = ["blocking"] }
```

## Rows and Values

Rows are `HashMap<String, serde_json::Value>`. Access column values with:

```rust
let row = db.query_one("SELECT name, score FROM users WHERE id = $1", &[&1i64]).await?;

let name = row["name"].as_str().unwrap_or("");
let score = row["score"].as_f64().unwrap_or(0.0);
let count = row["count"].as_i64().unwrap_or(0);
let active = row["active"].as_bool().unwrap_or(false);
```

## Error Handling

```rust
use deltex::Error;

match db.query_one("SELECT * FROM users WHERE id = $1", &[&999i64]).await {
    Ok(row) => println!("Found: {:?}", row),
    Err(Error::NoRows) => println!("User not found"),
    Err(Error::Database(msg)) => println!("DB error: {}", msg),
    Err(e) => println!("Other error: {}", e),
}
```

## Running the Example

```bash
export DELTEX_API_KEY="dtx_k_..."
cargo run --example basic --features async
```

## License

MIT

---

## Common Patterns

### Error handling

```rust
use deltex::Error;

match client.query_one("SELECT * FROM users WHERE id = $1", &[&42i64]).await {
    Ok(row) => println!("found: {:?}", row),
    Err(Error::NoRows) => println!("no such user"),
    Err(Error::Database(msg)) => eprintln!("engine error: {msg}"),
    Err(e) => return Err(e),
}
```

### Transactions

```rust
client.transaction(|tx| async move {
    tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", &[&100i64, &1i64]).await?;
    tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", &[&100i64, &2i64]).await?;
    Ok(())
}).await?;
```

### Strong consistency

```rust
let rows = client.strong()
    .query("SELECT balance FROM accounts WHERE id = $1", &[&account_id])
    .await?;
```

### Deserialize into structs

```rust
#[derive(serde::Deserialize)]
struct User { id: i64, name: String, score: f64 }

let users: Vec<User> = client
    .query_as("SELECT id, name, score FROM users LIMIT 10", &[])
    .await?;
```

## SDK Version

`v1.3.1` — see [CHANGELOG.md](../../CHANGELOG.md) for history.

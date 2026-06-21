//! Official Rust client for Deltex — edge-native SQL database.
//!
// ... (existing docs) ...

use std::collections::HashMap;
use std::fmt;

pub use serde_json::Value;

/// Current SDK version.
pub const VERSION: &str = "1.3.1";

mod tests;


// ===== Error Types =====

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    Database(String),
    #[error("No rows returned")]
    NoRows,
    #[error("Serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A database row — map of column name → JSON value.
pub type Row = HashMap<String, Value>;

// ===== Options =====

/// Connection options for the Deltex client.
#[derive(Clone, Debug)]
pub struct Options {
    /// API key (Bearer token). Defaults to `DELTEX_API_KEY` env var.
    pub api_key: String,
    /// Base URL of the Deltex service. Defaults to `https://db.deltex.dev`.
    pub base_url: String,
    /// Write mode: "sync" (default), "async", or "edge".
    pub write_mode: String,
    /// Request timeout in seconds. Defaults to 30.
    pub timeout_secs: u64,
}

impl Options {
    /// Create options from environment variables.
    ///
    /// Required: `DELTEX_API_KEY`
    /// Optional: `DELTEX_URL` (default: `https://db.deltex.dev`), `DELTEX_WRITE_MODE`
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("DELTEX_API_KEY")
            .map_err(|_| Error::Config("DELTEX_API_KEY env var not set".to_string()))?;
        let base_url = std::env::var("DELTEX_URL")
            .unwrap_or_else(|_| "https://db.deltex.dev".to_string());
        let write_mode = std::env::var("DELTEX_WRITE_MODE")
            .unwrap_or_else(|_| "sync".to_string());
        Ok(Self { api_key, base_url, write_mode, timeout_secs: 30 })
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = key.into(); self
    }
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into(); self
    }
    pub fn with_write_mode(mut self, mode: impl Into<String>) -> Self {
        self.write_mode = mode.into(); self
    }
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs; self
    }
}

// ===== Bind parameter trait =====

/// A value that can be bound as a query parameter.
pub trait Bind: Send + Sync {
    fn to_sql_string(&self) -> String;
}

impl Bind for i32    { fn to_sql_string(&self) -> String { self.to_string() } }
impl Bind for i64    { fn to_sql_string(&self) -> String { self.to_string() } }
impl Bind for u32    { fn to_sql_string(&self) -> String { self.to_string() } }
impl Bind for u64    { fn to_sql_string(&self) -> String { self.to_string() } }
impl Bind for f32    { fn to_sql_string(&self) -> String { self.to_string() } }
impl Bind for f64    { fn to_sql_string(&self) -> String { self.to_string() } }
impl Bind for bool   { fn to_sql_string(&self) -> String { if *self { "TRUE".to_string() } else { "FALSE".to_string() } } }
impl Bind for str    { fn to_sql_string(&self) -> String { format!("'{}'", self.replace('\'', "''")) } }
impl Bind for String { fn to_sql_string(&self) -> String { format!("'{}'", self.replace('\'', "''")) } }
impl<T: Bind> Bind for Option<T> {
    fn to_sql_string(&self) -> String {
        match self { Some(v) => v.to_sql_string(), None => "NULL".to_string() }
    }
}
// Allow `&T` to bind when `T: Bind + Sized` (covers `&i64`, `&&str` via &&str → &str → str etc.)
impl<T: Bind + ?Sized> Bind for &T {
    fn to_sql_string(&self) -> String { (**self).to_sql_string() }
}

fn bind_params(sql: &str, params: &[&dyn Bind]) -> String {
    let mut result = sql.to_string();
    for (i, param) in params.iter().enumerate() {
        let placeholder = format!("${}", i + 1);
        result = result.replace(&placeholder, &param.to_sql_string());
    }
    result
}

// ===== Query response types =====

#[derive(Debug, serde::Deserialize)]
struct QueryResponse {
    success: bool,
    message: Option<String>,
    #[allow(dead_code)]
    columns: Option<Vec<String>>,
    rows: Option<Vec<Row>>,
    affected_rows: Option<usize>,
}

// ===== Client =====

/// Async Deltex client.
#[derive(Clone)]
pub struct Client {
    opts: Options,
    http: reqwest::Client,
    query_url: String,
    txn_url: String,
    strong: bool,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("base_url", &self.opts.base_url)
            .field("write_mode", &self.opts.write_mode)
            .finish()
    }
}

impl Client {
    /// Create a new client with the given options.
    pub fn connect(opts: Options) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(opts.timeout_secs))
            .build()
            .map_err(Error::Http)?;
        let query_url = format!("{}/v1/query", opts.base_url);
        let txn_url = format!("{}/v1/transaction", opts.base_url);
        Ok(Self { opts, http, query_url, txn_url, strong: false })
    }

    /// Create a client from environment variables (`DELTEX_API_KEY`, `DELTEX_URL`).
    pub fn from_env() -> Result<Self> {
        Self::connect(Options::from_env()?)
    }

    async fn run_query(&self, sql: &str) -> Result<QueryResponse> {
        let body = serde_json::json!({ "sql": sql });
        let mut req = self.http
            .post(&self.query_url)
            .header("Authorization", format!("Bearer {}", self.opts.api_key))
            .header("Content-Type", "application/json")
            .header("X-Write-Mode", &self.opts.write_mode);
        if self.strong {
            req = req.header("X-Consistency", "strong");
        }
        let resp = req.json(&body).send().await?;
        let qr: QueryResponse = resp.json().await?;
        if !qr.success {
            return Err(Error::Database(qr.message.unwrap_or_else(|| "Unknown error".to_string())));
        }
        Ok(qr)
    }

    /// Execute a query returning multiple rows.
    ///
    /// Parameters are bound positionally: `$1`, `$2`, etc.
    ///
    /// ```no_run
    /// # async fn run(db: &deltex::Client) -> deltex::Result<()> {
    /// let rows = db.query("SELECT * FROM users WHERE age > $1", &[&18]).await?;
    /// # Ok(()) }
    /// ```
    pub async fn query(&self, sql: &str, params: &[&dyn Bind]) -> Result<Vec<Row>> {
        let sql = bind_params(sql, params);
        let qr = self.run_query(&sql).await?;
        Ok(qr.rows.unwrap_or_default())
    }

    /// Execute a query returning exactly one row.
    /// Returns `Error::NoRows` if no rows are returned.
    pub async fn query_one(&self, sql: &str, params: &[&dyn Bind]) -> Result<Row> {
        let rows = self.query(sql, params).await?;
        rows.into_iter().next().ok_or(Error::NoRows)
    }

    /// Execute a query returning an optional row.
    pub async fn query_opt(&self, sql: &str, params: &[&dyn Bind]) -> Result<Option<Row>> {
        let rows = self.query(sql, params).await?;
        Ok(rows.into_iter().next())
    }

    /// Execute a statement (INSERT/UPDATE/DELETE) and return the number of affected rows.
    pub async fn execute(&self, sql: &str, params: &[&dyn Bind]) -> Result<usize> {
        let sql = bind_params(sql, params);
        let qr = self.run_query(&sql).await?;
        Ok(qr.affected_rows.unwrap_or(0))
    }

    /// Execute a raw SQL string with no parameter binding.
    pub async fn execute_raw(&self, sql: &str) -> Result<Vec<Row>> {
        let qr = self.run_query(sql).await?;
        Ok(qr.rows.unwrap_or_default())
    }

    /// Run multiple statements in a transaction.
    ///
    /// The closure receives a [`Transaction`]. Statements issued on it are
    /// collected and committed atomically in a single round-trip when the
    /// closure returns `Ok`. If the closure returns `Err`, no statements are
    /// sent (the transaction is rolled back). Reads should be issued on the
    /// client directly, not the transaction handle.
    ///
    /// ```no_run
    /// # async fn run(db: &deltex::Client) -> deltex::Result<()> {
    /// db.transaction(|tx| async move {
    ///     tx.execute("INSERT INTO events (name) VALUES ($1)", &[&"signup"]).await?;
    ///     tx.execute("UPDATE stats SET count = count + 1 WHERE key = $1", &[&"signups"]).await?;
    ///     Ok(())
    /// }).await?;
    /// # Ok(()) }
    /// ```
    pub async fn transaction<F, Fut>(&self, f: F) -> Result<()>
    where
        F: FnOnce(Transaction) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let stmts = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let tx = Transaction { stmts: stmts.clone() };
        f(tx).await?;
        let collected = std::mem::take(&mut *stmts.lock().unwrap());
        if collected.is_empty() {
            return Ok(());
        }
        self.commit(TxnBuilder { stmts: collected }).await?;
        Ok(())
    }

    /// Run a transaction using a builder pattern (explicit statement collection).
    ///
    /// ```no_run
    /// # async fn run(db: &deltex::Client) -> deltex::Result<()> {
    /// let mut txn = db.begin();
    /// txn.add("INSERT INTO log (msg) VALUES ($1)", &[&"start"]);
    /// txn.add("UPDATE counters SET n = n + 1 WHERE id = $1", &[&"requests"]);
    /// db.commit(txn).await?;
    /// # Ok(()) }
    /// ```
    pub fn begin(&self) -> TxnBuilder {
        TxnBuilder { stmts: Vec::new() }
    }

    pub async fn commit(&self, txn: TxnBuilder) -> Result<Vec<Vec<Row>>> {
        let body = serde_json::json!({
            "statements": txn.stmts,
            "isolation": "SERIALIZABLE"
        });
        let resp = self.http
            .post(&self.txn_url)
            .header("Authorization", format!("Bearer {}", self.opts.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        let raw: serde_json::Value = resp.json().await?;
        if let Some(err) = raw.get("error") {
            return Err(Error::Database(err.as_str().unwrap_or("transaction failed").to_string()));
        }
        // Parse results array
        let results = raw.get("results")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(|item| {
                item.get("rows").and_then(|r| serde_json::from_value(r.clone()).ok())
            }).collect())
            .unwrap_or_default();
        Ok(results)
    }

    /// Execute a query and deserialize each row into `T`.
    ///
    /// ```no_run
    /// # async fn run(db: &deltex::Client) -> deltex::Result<()> {
    /// #[derive(serde::Deserialize)]
    /// struct User { id: i64, name: String }
    /// let users: Vec<User> = db.query_as("SELECT id, name FROM users", &[]).await?;
    /// # Ok(()) }
    /// ```
    pub async fn query_as<T: serde::de::DeserializeOwned>(
        &self,
        sql: &str,
        params: &[&dyn Bind],
    ) -> Result<Vec<T>> {
        let rows = self.query(sql, params).await?;
        rows.into_iter()
            .map(|row| {
                serde_json::to_value(row)
                    .and_then(serde_json::from_value)
                    .map_err(Error::Serialize)
            })
            .collect()
    }

    /// Return a client whose reads use strong consistency (`X-Consistency: strong`),
    /// bypassing the read cache.
    pub fn strong(&self) -> Self {
        let mut c = self.clone();
        c.strong = true;
        c
    }

    /// Return a client with a specific write mode.
    pub fn with_write_mode(&self, mode: &str) -> Self {
        let mut c = self.clone();
        c.opts.write_mode = mode.to_string();
        c
    }
}

// ===== Transaction =====

/// Transaction handle passed to the closure in [`Client::transaction`].
///
/// Mutating statements are collected as they are issued and sent to the engine
/// atomically once the closure returns `Ok`. If the closure returns `Err`,
/// nothing is sent.
pub struct Transaction {
    stmts: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Transaction {
    /// Add a parameterized statement to the transaction.
    pub async fn execute(&self, sql: &str, params: &[&dyn Bind]) -> Result<usize> {
        self.stmts.lock().unwrap().push(bind_params(sql, params));
        Ok(0)
    }

    /// Add a raw statement (no parameter binding) to the transaction.
    pub async fn execute_raw(&self, sql: &str) -> Result<usize> {
        self.stmts.lock().unwrap().push(sql.to_string());
        Ok(0)
    }
}

// ===== TxnBuilder =====

/// Explicit transaction builder for collecting statements before committing.
pub struct TxnBuilder {
    stmts: Vec<String>,
}

impl TxnBuilder {
    /// Add a statement to the transaction.
    pub fn add(&mut self, sql: &str, params: &[&dyn Bind]) -> &mut Self {
        self.stmts.push(bind_params(sql, params));
        self
    }

    /// Add a raw SQL statement with no parameter binding.
    pub fn add_raw(&mut self, sql: &str) -> &mut Self {
        self.stmts.push(sql.to_string());
        self
    }
}

// ===== Blocking Client =====
#[cfg(feature = "blocking")]
pub mod blocking {
    use super::*;

    /// Synchronous (blocking) Deltex client.
    pub struct Client {
        opts: Options,
        http: reqwest::blocking::Client,
        query_url: String,
        txn_url: String,
    }

    impl Client {
        pub fn connect(opts: Options) -> Result<Self> {
            let http = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(opts.timeout_secs))
                .build()
                .map_err(Error::Http)?;
            let query_url = format!("{}/v1/query", opts.base_url);
            let txn_url = format!("{}/v1/transaction", opts.base_url);
            Ok(Self { opts, http, query_url, txn_url })
        }

        pub fn from_env() -> Result<Self> {
            Self::connect(Options::from_env()?)
        }

        fn run_query(&self, sql: &str) -> Result<QueryResponse> {
            let body = serde_json::json!({ "sql": sql });
            let resp = self.http
                .post(&self.query_url)
                .header("Authorization", format!("Bearer {}", self.opts.api_key))
                .header("Content-Type", "application/json")
                .header("X-Write-Mode", &self.opts.write_mode)
                .json(&body)
                .send()?;
            let qr: QueryResponse = resp.json()?;
            if !qr.success {
                return Err(Error::Database(qr.message.unwrap_or_else(|| "Unknown error".to_string())));
            }
            Ok(qr)
        }

        pub fn query(&self, sql: &str, params: &[&dyn Bind]) -> Result<Vec<Row>> {
            let sql = bind_params(sql, params);
            Ok(self.run_query(&sql)?.rows.unwrap_or_default())
        }

        pub fn query_one(&self, sql: &str, params: &[&dyn Bind]) -> Result<Row> {
            self.query(sql, params)?.into_iter().next().ok_or(Error::NoRows)
        }

        pub fn query_opt(&self, sql: &str, params: &[&dyn Bind]) -> Result<Option<Row>> {
            Ok(self.query(sql, params)?.into_iter().next())
        }

        pub fn execute(&self, sql: &str, params: &[&dyn Bind]) -> Result<usize> {
            let sql = bind_params(sql, params);
            Ok(self.run_query(&sql)?.affected_rows.unwrap_or(0))
        }

        pub fn execute_raw(&self, sql: &str) -> Result<Vec<Row>> {
            Ok(self.run_query(sql)?.rows.unwrap_or_default())
        }

        pub fn begin(&self) -> TxnBuilder {
            TxnBuilder { stmts: Vec::new() }
        }

        pub fn commit(&self, txn: TxnBuilder) -> Result<Vec<Vec<Row>>> {
            let body = serde_json::json!({
                "statements": txn.stmts,
                "isolation": "SERIALIZABLE"
            });
            let resp = self.http
                .post(&self.txn_url)
                .header("Authorization", format!("Bearer {}", self.opts.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()?;
            let raw: serde_json::Value = resp.json()?;
            if let Some(err) = raw.get("error") {
                return Err(Error::Database(err.as_str().unwrap_or("transaction failed").to_string()));
            }
            let results = raw.get("results")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|item| {
                    item.get("rows").and_then(|r| serde_json::from_value(r.clone()).ok())
                }).collect())
                .unwrap_or_default();
            Ok(results)
        }
    }
}

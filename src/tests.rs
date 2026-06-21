//! Unit tests for the Deltex Rust SDK.

#[cfg(test)]
mod tests {
    use super::super::*;

    // ── Bind trait tests ──────────────────────────────────────────────────────

    #[test]
    fn test_bind_integer() {
        assert_eq!(42i64.to_sql_string(), "42");
        assert_eq!((-7i32).to_sql_string(), "-7");
        assert_eq!(0u32.to_sql_string(), "0");
    }

    #[test]
    fn test_bind_float() {
        assert_eq!(3.14f64.to_sql_string(), "3.14");
        assert_eq!((-1.5f32).to_sql_string(), "-1.5");
    }

    #[test]
    fn test_bind_bool() {
        assert_eq!(true.to_sql_string(), "TRUE");
        assert_eq!(false.to_sql_string(), "FALSE");
    }

    #[test]
    fn test_bind_string_simple() {
        assert_eq!("hello".to_sql_string(), "'hello'");
        assert_eq!("world".to_string().to_sql_string(), "'world'");
    }

    #[test]
    fn test_bind_string_single_quote_escape() {
        // SQL injection prevention: single quotes are doubled
        assert_eq!("O'Brien".to_sql_string(), "'O''Brien'");
        assert_eq!("it's a test".to_sql_string(), "'it''s a test'");
    }

    #[test]
    fn test_bind_option_some() {
        let v: Option<i64> = Some(42);
        assert_eq!(v.to_sql_string(), "42");
        let s: Option<&str> = Some("hello");
        assert_eq!(s.to_sql_string(), "'hello'");
    }

    #[test]
    fn test_bind_option_none() {
        let v: Option<i64> = None;
        assert_eq!(v.to_sql_string(), "NULL");
        let s: Option<&str> = None;
        assert_eq!(s.to_sql_string(), "NULL");
    }

    // ── Parameter binding tests ───────────────────────────────────────────────

    #[test]
    fn test_bind_params_single() {
        let sql = bind_params("SELECT * FROM t WHERE id = $1", &[&42i64]);
        assert_eq!(sql, "SELECT * FROM t WHERE id = 42");
    }

    #[test]
    fn test_bind_params_multiple() {
        let sql = bind_params(
            "INSERT INTO t (a, b, c) VALUES ($1, $2, $3)",
            &[&1i64, &"hello", &true],
        );
        assert_eq!(sql, "INSERT INTO t (a, b, c) VALUES (1, 'hello', TRUE)");
    }

    #[test]
    fn test_bind_params_string_escaping() {
        let sql = bind_params(
            "SELECT * FROM t WHERE name = $1",
            &[&"O'Reilly"],
        );
        assert_eq!(sql, "SELECT * FROM t WHERE name = 'O''Reilly'");
    }

    #[test]
    fn test_bind_params_null() {
        let v: Option<i64> = None;
        let sql = bind_params("UPDATE t SET col = $1 WHERE id = $2", &[&v, &5i64]);
        assert_eq!(sql, "UPDATE t SET col = NULL WHERE id = 5");
    }

    #[test]
    fn test_bind_params_positional_order() {
        // $1 then $2 — order matters
        let sql = bind_params("$1 $2 $1", &[&"first", &"second"]);
        assert_eq!(sql, "'first' 'second' 'first'");
    }

    #[test]
    fn test_bind_params_float() {
        let sql = bind_params("SELECT * FROM t WHERE score > $1", &[&9.5f64]);
        assert_eq!(sql, "SELECT * FROM t WHERE score > 9.5");
    }

    // ── Options tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_options_from_env_missing_key() {
        // Without DELTEX_API_KEY set, should error
        let old = std::env::var("DELTEX_API_KEY").ok();
        std::env::remove_var("DELTEX_API_KEY");
        let result = Options::from_env();
        assert!(result.is_err());
        if let Some(key) = old { std::env::set_var("DELTEX_API_KEY", key); }
    }

    #[test]
    fn test_options_from_env_with_key() {
        let old = std::env::var("DELTEX_API_KEY").ok();
        std::env::set_var("DELTEX_API_KEY", "test_key_123");
        let opts = Options::from_env().unwrap();
        assert_eq!(opts.api_key, "test_key_123");
        assert_eq!(opts.base_url, "https://db.deltex.dev");
        assert_eq!(opts.write_mode, "sync");
        assert_eq!(opts.timeout_secs, 30);
        if let Some(k) = old { std::env::set_var("DELTEX_API_KEY", k); }
        else { std::env::remove_var("DELTEX_API_KEY"); }
    }

    #[test]
    fn test_options_builder() {
        let opts = Options {
            api_key: "key".to_string(),
            base_url: "https://db.deltex.dev".to_string(),
            write_mode: "sync".to_string(),
            timeout_secs: 30,
        }
        .with_api_key("new_key")
        .with_url("https://custom.example.com")
        .with_write_mode("edge")
        .with_timeout(60);

        assert_eq!(opts.api_key, "new_key");
        assert_eq!(opts.base_url, "https://custom.example.com");
        assert_eq!(opts.write_mode, "edge");
        assert_eq!(opts.timeout_secs, 60);
    }

    // ── TxnBuilder tests ──────────────────────────────────────────────────────

    #[test]
    fn test_txn_builder_add() {
        let mut txn = TxnBuilder { stmts: Vec::new() };
        txn.add("INSERT INTO t (id) VALUES ($1)", &[&1i64]);
        txn.add("UPDATE t SET x = $1 WHERE id = $2", &[&42i64, &1i64]);
        assert_eq!(txn.stmts.len(), 2);
        assert_eq!(txn.stmts[0], "INSERT INTO t (id) VALUES (1)");
        assert_eq!(txn.stmts[1], "UPDATE t SET x = 42 WHERE id = 1");
    }

    #[test]
    fn test_txn_builder_add_raw() {
        let mut txn = TxnBuilder { stmts: Vec::new() };
        txn.add_raw("SELECT 1");
        txn.add_raw("SELECT 2");
        assert_eq!(txn.stmts.len(), 2);
        assert_eq!(txn.stmts[0], "SELECT 1");
        assert_eq!(txn.stmts[1], "SELECT 2");
    }

    #[test]
    fn test_txn_builder_chaining() {
        let mut txn = TxnBuilder { stmts: Vec::new() };
        txn.add("INSERT INTO a VALUES ($1)", &[&1i64])
           .add("INSERT INTO b VALUES ($1)", &[&2i64])
           .add_raw("SELECT 42");
        assert_eq!(txn.stmts.len(), 3);
    }

    // ── Transaction handle (regression: must COLLECT, not silently drop) ───────

    #[tokio::test]
    async fn test_transaction_handle_collects_statements() {
        use std::sync::{Arc, Mutex};
        let stmts = Arc::new(Mutex::new(Vec::<String>::new()));
        let tx = Transaction { stmts: stmts.clone() };
        tx.execute("INSERT INTO t (id) VALUES ($1)", &[&1i64]).await.unwrap();
        tx.execute_raw("UPDATE t SET x = 1").await.unwrap();
        let collected = stmts.lock().unwrap();
        assert_eq!(collected.len(), 2, "transaction handle must collect issued statements");
        assert_eq!(collected[0], "INSERT INTO t (id) VALUES (1)");
        assert_eq!(collected[1], "UPDATE t SET x = 1");
    }
}

-- no-transaction
-- SQLite runtime defaults are also applied to every pooled connection in
-- crates/ersatzrs-infra/src/db.rs. This migration persists WAL mode for
-- existing databases and applies the connection-local defaults while running.
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 30000;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

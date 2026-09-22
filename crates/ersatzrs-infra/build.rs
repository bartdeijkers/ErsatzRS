// `sqlx::migrate!` embeds the migration directories into the binary at compile
// time, but a proc macro cannot register a directory as a Cargo input on stable
// Rust. Without these directives, adding or editing a `.sql` file changes none
// of this crate's tracked inputs: Cargo reuses the cached build and the
// compiled `Migrator` silently keeps the previous migration set, so the new
// migration never runs and the database looks unchanged after a restart.
//
// Track the SQLite migrator root used by `db.rs`. Do not register removed
// directories: a missing watched path makes Cargo rerun this build every time.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}

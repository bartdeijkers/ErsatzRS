// `sqlx::migrate!` embeds the migration directories into the binary at compile
// time, but a proc macro cannot register a directory as a Cargo input on stable
// Rust. Without these directives, adding or editing a `.sql` file changes none
// of this crate's tracked inputs: Cargo reuses the cached build and the
// compiled `Migrator` silently keeps the previous migration set, so the new
// migration never runs and the database looks unchanged after a restart.
//
// Both migrator roots from `db.rs` are listed explicitly rather than relying on
// a recursive scan of the parent directory.
fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
    println!("cargo:rerun-if-changed=../../migrations/mysql");
}

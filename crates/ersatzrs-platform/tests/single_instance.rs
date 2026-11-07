//! Real OS lock ownership; runs natively on Linux and Windows without media.

use std::{
    fs::{self, File, Permissions},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ersatzrs_platform::single_instance::{Error, Guard, LOCK_FILE_NAME};

struct Folder(PathBuf);

impl Folder {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ersatzrs-instance-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn stale_file_is_reusable_without_truncating_its_contents() {
    let folder = Folder::new();
    let path = folder.0.join(LOCK_FILE_NAME);
    fs::write(&path, b"retained stale contents").unwrap();
    let owner = Guard::acquire(&folder.0).unwrap();
    assert!(matches!(
        Guard::acquire(&folder.0),
        Err(Error::AlreadyRunning)
    ));
    drop(owner);
    let replacement = Guard::acquire(&folder.0).unwrap();
    // Windows byte-range locks also exclude reads by unrelated handles.
    drop(replacement);
    assert_eq!(fs::read(path).unwrap(), b"retained stale contents");
}

#[test]
fn separate_state_folders_have_independent_owners() {
    let first = Folder::new();
    let second = Folder::new();
    let _first_owner = Guard::acquire(&first.0).unwrap();
    let _second_owner = Guard::acquire(&second.0).unwrap();
}

#[test]
fn unavailable_folder_is_not_reported_as_another_instance() {
    let folder = Folder::new();
    let file = folder.0.join("not-a-folder");
    fs::write(&file, b"unrelated data").unwrap();
    assert!(matches!(Guard::acquire(&file), Err(Error::Unavailable(_))));
    assert_eq!(fs::read(file).unwrap(), b"unrelated data");
}

struct RestorePermissions(PathBuf, Permissions);

impl Drop for RestorePermissions {
    fn drop(&mut self) {
        fs::set_permissions(&self.0, self.1.clone()).unwrap();
    }
}

#[test]
fn read_only_existing_file_uses_the_fallback_and_keeps_exclusion() {
    let folder = Folder::new();
    let path = folder.0.join(LOCK_FILE_NAME);
    fs::write(&path, b"do not truncate").unwrap();
    let writable = fs::metadata(&path).unwrap().permissions();
    let restore = RestorePermissions(path.clone(), writable.clone());
    let mut read_only = writable;
    read_only.set_readonly(true);
    fs::set_permissions(&path, read_only).unwrap();
    assert!(File::options().write(true).open(&path).is_err());
    let owner = Guard::acquire(&folder.0).unwrap();
    assert!(matches!(
        Guard::acquire(&folder.0),
        Err(Error::AlreadyRunning)
    ));
    drop(owner);
    drop(Guard::acquire(&folder.0).unwrap());
    drop(restore);
    assert_eq!(fs::read(path).unwrap(), b"do not truncate");
}

struct Holder(Child);

impl Holder {
    fn start(folder: &Path) -> Self {
        fs::write(folder.join("child-request"), b"hold lock").unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "child_lock_holder", "--ignored", "--nocapture"])
            .current_dir(folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut holder = Self(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !folder.join("child-ready").exists() {
            assert!(
                holder.0.try_wait().unwrap().is_none(),
                "child exited before ownership"
            );
            assert!(
                Instant::now() < deadline,
                "child failed to acquire ownership"
            );
            thread::sleep(Duration::from_millis(10));
        }
        holder
    }
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn competing_process_is_refused_and_killed_owner_does_not_leave_a_stale_lock() {
    let folder = Folder::new();
    let mut holder = Holder::start(&folder.0);
    assert!(matches!(
        Guard::acquire(&folder.0),
        Err(Error::AlreadyRunning)
    ));
    holder.0.kill().unwrap();
    assert!(!holder.0.wait().unwrap().success());
    let _replacement = Guard::acquire(&folder.0).unwrap();
}

#[test]
fn owner_exit_releases_the_lock_without_removing_the_file() {
    let folder = Folder::new();
    let mut holder = Holder::start(&folder.0);
    drop(holder.0.stdin.take());
    assert!(holder.0.wait().unwrap().success());
    assert!(folder.0.join(LOCK_FILE_NAME).is_file());
    let _replacement = Guard::acquire(&folder.0).unwrap();
}

#[test]
#[ignore = "requires ERSATZRS_INSTANCE_TEST_WEB_BIN pointing to the current real server executable"]
#[allow(
    clippy::disallowed_methods,
    reason = "this opt-in integration test receives an exact candidate executable through its documented test input"
)]
fn actual_server_refuses_competing_state_before_database_creation() {
    let binary = std::env::var_os("ERSATZRS_INSTANCE_TEST_WEB_BIN")
        .expect("supply the current compiled server executable");
    let folder = Folder::new();
    let _owner = Guard::acquire(&folder.0).unwrap();
    let database = folder.0.join("must-not-be-created.db");
    let url = format!("sqlite:{}", database.display());
    let mut child = Holder(
        Command::new(binary)
            .env("ERSATZRS_APP_STATE_DIR", &folder.0)
            .env("ERSATZRS_APP_DATA_DIR", folder.0.join("cache"))
            .env("DATABASE_URL", &url)
            .env("ERSATZRS_DATABASE__URL", &url)
            .env("ErsatzRS_DATABASE__URL", &url)
            .env("ERSATZRS_FFMPEG_DOWNLOAD", "false")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "server ignored the competing owner"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(1));
    let mut stderr = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.contains("another ErsatzRS instance is already running"),
        "{stderr}"
    );
    assert!(
        !database.exists(),
        "a refused server must not open the database"
    );
    assert!(
        !folder.0.join("cache").exists(),
        "a refused server must not initialize runtimes"
    );
}

#[test]
#[ignore = "subprocess fixture invoked by the two ownership tests with an isolated working directory"]
fn child_lock_holder() {
    let folder = std::env::current_dir().unwrap();
    assert!(
        folder.join("child-request").is_file(),
        "only run via the parent fixture"
    );
    let _owner = Guard::acquire(&folder).unwrap();
    fs::write(folder.join("child-ready"), b"locked").unwrap();
    let _ = std::io::stdin().read(&mut [0]);
}

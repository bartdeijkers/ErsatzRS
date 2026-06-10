//! ErsatzRS Windows tray launcher.
//!
//! A faithful port of the archived `ErsatzTV-Windows` helper: it puts an icon in
//! the system tray, starts `ersatzrs-web.exe` in the background (no console
//! window), and offers "Open Web UI", "Show Logs", and a clean "Exit". Exit
//! delivers a console Ctrl+C to the server so its async runtime runs the
//! graceful-shutdown path (see `ersatzrs-web` `host::bootstrap::shutdown_signal`).
//!
//! On non-Windows targets this is a no-op `main` so `cargo build --workspace`
//! stays green on the Linux CI; the launcher is only meaningful on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    println!("ErsatzRS launcher is Windows-only; run ersatzrs-web directly on this platform.");
}

#[cfg(windows)]
fn main() {
    windows_launcher::run();
}

#[cfg(windows)]
mod windows_launcher {
    use std::io;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use tray_item::{IconSource, TrayItem};
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler, CTRL_C_EVENT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    /// The server's hardcoded listen port (`ersatzrs-web` `LISTEN_PORT`).
    const WEB_URL: &str = "http://localhost:8409";
    /// Server binary co-located with the launcher in the release bundle.
    const SERVER_EXE: &str = "ersatzrs-web.exe";
    /// `CREATE_NO_WINDOW` process creation flag (no console window for the child).
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    /// How long Exit waits for the server to drain after the Ctrl+C before it
    /// force-kills. Slightly longer than the server's own drain budget.
    const SHUTDOWN_WAIT: Duration = Duration::from_secs(15);
    /// Poll cadence while waiting for the server to exit.
    const SHUTDOWN_POLL: Duration = Duration::from_millis(100);

    enum Message {
        OpenWebUi,
        ShowLogs,
        Exit,
    }

    pub fn run() {
        let exe_dir = launcher_dir();

        // The launcher is a GUI-subsystem app with no console, so a panic here
        // would vanish silently and leave the user with no tray and no message.
        // Surface every startup failure in a dialog instead.
        let mut child = match spawn_server(&exe_dir) {
            Ok(child) => child,
            Err(error) => {
                show_error(&format!(
                    "ErsatzRS could not start its server ({SERVER_EXE}).\n\n{error}"
                ));
                std::process::exit(1);
            }
        };

        let mut tray = match TrayItem::new("ErsatzRS", IconSource::Resource("ersatzrs-icon")) {
            Ok(tray) => tray,
            Err(error) => {
                show_error(&format!(
                    "ErsatzRS could not create its tray icon.\n\n{error}"
                ));
                stop_server(&mut child);
                std::process::exit(1);
            }
        };

        // `tray-item`'s menu callbacks must be `Fn() + Send + Sync + 'static`, so
        // use a `SyncSender` (the `mpsc::Sender` returned by `channel` is not
        // `Sync`). This mirrors tray-item's documented usage.
        let (tx, rx) = mpsc::sync_channel::<Message>(16);

        // NOTE: a visual separator before "Exit" would go here via
        // `tray.inner_mut().add_separator()`; omitted to keep the menu API
        // version-agnostic. Add it once the tray-item version is pinned.
        //
        // Built in an immediately-invoked closure so a single `?`-chain handles
        // all three menu items and the closure (and its `&mut tray` borrow) is
        // dropped right after, leaving `tray` free to outlive it.
        let menu = (|| -> Result<(), tray_item::TIError> {
            let open_tx = tx.clone();
            tray.add_menu_item("Open Web UI", move || {
                let _ = open_tx.send(Message::OpenWebUi);
            })?;
            let logs_tx = tx.clone();
            tray.add_menu_item("Show Logs", move || {
                let _ = logs_tx.send(Message::ShowLogs);
            })?;
            let exit_tx = tx.clone();
            tray.add_menu_item("Exit", move || {
                let _ = exit_tx.send(Message::Exit);
            })?;
            Ok(())
        })();
        if let Err(error) = menu {
            show_error(&format!(
                "ErsatzRS could not build its tray menu.\n\n{error}"
            ));
            stop_server(&mut child);
            std::process::exit(1);
        }

        // `tray-item` runs the Win32 message pump on its own thread, so we just
        // block on the channel here.
        loop {
            match rx.recv() {
                Ok(Message::OpenWebUi) => {
                    let _ = open::that(WEB_URL);
                }
                Ok(Message::ShowLogs) => {
                    let logs = log_dir();
                    let _ = std::fs::create_dir_all(&logs);
                    let _ = open::that(&logs);
                }
                // Exit, or the senders were all dropped: stop the server and quit.
                Ok(Message::Exit) | Err(_) => {
                    stop_server(&mut child);
                    break;
                }
            }
        }
    }

    /// Directory the launcher exe lives in. The server, ffmpeg, and site assets
    /// are all staged next to it in the release zip.
    fn launcher_dir() -> PathBuf {
        process_path::get_executable_path()
            .as_deref()
            .and_then(Path::parent)
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    /// Spawn `ersatzrs-web.exe` with no console window, pointing it at the
    /// bundled FFmpeg binaries and Leptos hydration assets when present so the
    /// server is fully self-contained (no reliance on system PATH).
    fn spawn_server(exe_dir: &Path) -> io::Result<Child> {
        let server = exe_dir.join(SERVER_EXE);
        let mut command = Command::new(&server);
        command
            .current_dir(exe_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);

        // Env var names match ersatzrs-ffmpeg's path resolution (version.rs).
        set_env_if_file(
            &mut command,
            "ERSATZRS_FFMPEG__PATH",
            exe_dir.join("ffmpeg.exe"),
        );
        set_env_if_file(
            &mut command,
            "ERSATZRS_FFPROBE__PATH",
            exe_dir.join("ffprobe.exe"),
        );

        // Point Leptos at the bundled hydration assets (staged under `site/`) so
        // the admin UI is interactive. The release server binary is built with
        // plain `cargo build` (no cargo-leptos), so it has no compile-time
        // LEPTOS_OUTPUT_NAME default; without it the env-based config falls
        // back to an empty output name and the shell never emits the
        // hydration scripts.
        let site = exe_dir.join("site");
        if site.is_dir() {
            command.env("LEPTOS_SITE_ROOT", &site);
            command.env("LEPTOS_SITE_PKG_DIR", "pkg");
            if std::env::var_os("LEPTOS_OUTPUT_NAME").is_none() {
                command.env("LEPTOS_OUTPUT_NAME", "ersatzrs-web");
            }
        }

        command.spawn()
    }

    fn set_env_if_file(command: &mut Command, key: &str, path: PathBuf) {
        if path.is_file() {
            command.env(key, path);
        }
    }

    /// Resolve `%LOCALAPPDATA%\ErsatzRS\logs`, matching
    /// `ersatzrs_infra::logging::default_log_dir`.
    fn log_dir() -> PathBuf {
        directories::BaseDirs::new().map_or_else(
            || PathBuf::from("logs"),
            |dirs| dirs.data_local_dir().join("ErsatzRS").join("logs"),
        )
    }

    /// Send a console Ctrl+C to the server child so it drains gracefully, then
    /// wait (bounded) for it to exit, force-killing if it does not.
    ///
    /// The `unsafe` block is what the crate opts out of the workspace
    /// `unsafe_code = "forbid"` lint for: the Win32 console-control API has no
    /// safe wrapper. The sequence is the standard one used by ErsatzTV-Windows:
    ///   1. attach to the (windowless) child's console;
    ///   2. disable Ctrl+C handling for *this* process so the event we are about
    ///      to raise does not also terminate the launcher — and leave it disabled
    ///      through `FreeConsole` so a late-delivered event can't kill us;
    ///   3. raise `CTRL_C_EVENT` for the whole attached console group (the child);
    ///   4. detach.
    fn stop_server(child: &mut Child) {
        let pid = child.id();

        // SAFETY: these are raw Win32 FFI calls with no pointer arguments and no
        // memory-safety preconditions. `pid` is a valid child process id, and we
        // always detach (`FreeConsole`) after attaching. Failures are tolerated;
        // the bounded wait + kill below guarantees the child does not survive.
        unsafe {
            if AttachConsole(pid) != 0 {
                SetConsoleCtrlHandler(None, 1);
                let _ = GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0);
                let _ = FreeConsole();
            }
        }

        // Wait for the graceful drain, but never block forever: if AttachConsole
        // failed (no signal was sent) or the server ignores the Ctrl+C, the
        // earlier `child.wait()` would hang. Poll `try_wait` to a deadline, then
        // force-kill so we never leave an orphaned server behind.
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                Ok(None) => std::thread::sleep(SHUTDOWN_POLL),
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
        }
    }

    /// Show a modal error dialog. The launcher has no console (GUI subsystem), so
    /// this is the only way to surface a fatal startup failure to the user.
    fn show_error(message: &str) {
        let title = to_wide("ErsatzRS");
        let body = to_wide(message);

        // SAFETY: `MessageBoxW` reads two null-terminated UTF-16 strings; the
        // `to_wide` buffers are null-terminated and outlive the call. A null
        // owner HWND is valid (an unowned dialog).
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                body.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    /// Encode a string as a null-terminated UTF-16 buffer for the Win32 `W` APIs.
    fn to_wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

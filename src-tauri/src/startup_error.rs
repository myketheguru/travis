//! Last-ditch error reporting for failures that happen before any webview
//! exists. The webview-based health banner is the primary surface for
//! runtime issues — but if `setup()` itself fails (DB migration mismatch,
//! missing app-data dir, etc.) we have no UI yet, so the process would
//! just panic and Windows would silently kill the app. This module:
//!
//!   1. Writes the error to `<app data dir>/startup_error.log` so it's
//!      always recoverable by the user.
//!   2. On Windows, pops a native MessageBox so the user sees what went
//!      wrong instead of nothing.
//!   3. Calls `std::process::exit(1)` — caller never returns.

use std::io::Write;
use std::path::Path;

const TITLE: &str = "Travis couldn't start";

/// Show a native error dialog (Windows), append to the log file, then
/// exit the process with code 1. Never returns.
pub fn die(log_dir: Option<&Path>, message: impl AsRef<str>) -> ! {
    let body = message.as_ref();

    if let Some(dir) = log_dir {
        let _ = write_log(dir, body);
    }

    // Best-effort to stderr too — visible if the user launches from a
    // terminal.
    eprintln!("{TITLE}: {body}");

    #[cfg(windows)]
    show_message_box_windows(TITLE, body);

    std::process::exit(1);
}

fn write_log(dir: &Path, body: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("startup_error.log");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let now = chrono::Utc::now().to_rfc3339();
    writeln!(f, "[{now}] {body}")?;
    Ok(())
}

#[cfg(windows)]
fn show_message_box_windows(title: &str, body: &str) {
    // Direct FFI to user32.dll's MessageBoxW — avoids adding the
    // `windows`/`windows-sys` crate just for this one call.
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            hwnd: *const core::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            type_: u32,
        ) -> i32;
    }

    const MB_ICONERROR: u32 = 0x00000010;
    const MB_OK: u32 = 0x00000000;

    let title_w: Vec<u16> = OsStr::new(title).encode_wide().chain(once(0)).collect();
    let body_w: Vec<u16> = OsStr::new(body).encode_wide().chain(once(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null(),
            body_w.as_ptr(),
            title_w.as_ptr(),
            MB_ICONERROR | MB_OK,
        );
    }
}

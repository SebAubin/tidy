//! What differs between macOS and Windows for the person: the file manager, the trash, and
//! what they're called.

use std::path::Path;
use std::process::Command;

/// "Finder" or "File Explorer".
pub const FILE_MANAGER: &str = if cfg!(windows) { "File Explorer" } else { "Finder" };
/// "Trash" or "Recycle Bin".
pub const TRASH: &str = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
/// What the machine is called in sentences ("See what fills your computer"), the same everywhere.
pub const COMPUTER: &str = "computer";

/// The key that moves the selection to the trash: ⌘⌫ in the Finder, Delete in File Explorer.
pub const TRASH_SHORTCUT: &str = if cfg!(windows) { "delete" } else { "cmd+backspace" };

pub fn is_trash_key(ev: &rune::KeyEvent) -> bool {
    if cfg!(windows) {
        matches!(ev.key, rune::Key::Delete) && !ev.mods.cmd && !ev.mods.alt && !ev.mods.shift
    } else {
        matches!(ev.key, rune::Key::Backspace) && ev.mods.cmd
    }
}

pub fn reveal_label() -> String {
    format!("Show in {FILE_MANAGER}")
}

/// Shows items selected in the file manager.
pub fn reveal<P: AsRef<Path>>(paths: &[P]) {
    if paths.is_empty() {
        return;
    }
    if cfg!(windows) {
        // Explorer selects one item per window; the first is what the person picked.
        let _ = Command::new("explorer.exe").arg(format!("/select,{}", paths[0].as_ref().display())).spawn();
    } else {
        let _ = Command::new("open").arg("-R").args(paths.iter().map(|p| p.as_ref())).spawn();
    }
}

/// Opens a file or folder with its default app.
pub fn open(path: &Path) {
    if cfg!(windows) {
        let _ = Command::new("explorer.exe").arg(path).spawn();
    } else {
        let _ = Command::new("open").arg(path).spawn();
    }
}

/// Empties the Trash (Recycle Bin), as the system's own command does, without a prompt (Tidy
/// asked already).
pub fn empty_trash() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::{SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW};
        std::thread::spawn(|| unsafe {
            SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND);
        });
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("osascript").args(["-e", "tell application \"Finder\" to empty trash"]).spawn();
    }
}

/// Windows: the title bar's, taskbar's and Alt-Tab's icon, from the one built into tidy.exe
/// (resource 1), at this window's DPI. macOS takes the app's icon from its bundle.
pub fn set_window_icon(_w: &rune::WindowHandle) {
    #[cfg(windows)]
    {
        use raw_window_handle::RawWindowHandle;
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
        use windows_sys::Win32::UI::WindowsAndMessaging::{ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CXSMICON, SendMessageW, WM_SETICON};
        let _ = _w.with_raw_handle(|h| {
            let RawWindowHandle::Win32(h) = h.as_raw() else { return };
            let hwnd = h.hwnd.get() as windows_sys::Win32::Foundation::HWND;
            unsafe {
                let module = GetModuleHandleW(std::ptr::null());
                let dpi = GetDpiForWindow(hwnd).max(96);
                for (which, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
                    let size = GetSystemMetricsForDpi(metric, dpi);
                    // MAKEINTRESOURCE(1): the icon winresource builds in.
                    let icon = LoadImageW(module, 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR);
                    if !icon.is_null() {
                        SendMessageW(hwnd, WM_SETICON, which as usize, icon as isize);
                    }
                }
            }
        });
    }
}

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    if cfg!(windows) {
        let _ = Command::new("explorer.exe").arg(url).spawn();
    } else {
        let _ = Command::new("open").arg(url).spawn();
    }
}

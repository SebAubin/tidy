//! One Tidy at a time. Launch Services normally sees to it, but a forced second launch, or a
//! relaunch that starts before the old copy has finished quitting ("Quit & Reopen" after a
//! privacy permission), would run two side by side and show Tidy twice in the Dock.

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
use objc2_foundation::NSString;

/// Other running copies of this app (same bundle id), not counting this process.
fn others() -> Vec<objc2::rc::Retained<NSRunningApplication>> {
    let me = std::process::id() as i32;
    let Some(id) = NSRunningApplication::currentApplication().bundleIdentifier() else { return Vec::new() };
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(&id.to_string()))
        .iter()
        .filter(|a| a.processIdentifier() != me && !a.isTerminated())
        .collect()
}

/// Returns false when another Tidy is running (it's brought forward instead and this one should
/// exit). A copy that's still quitting is waited for, three seconds at most.
pub fn claim() -> bool {
    for _ in 0..30 {
        if others().is_empty() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    for other in others() {
        #[allow(deprecated)]
        other.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
    }
    false
}

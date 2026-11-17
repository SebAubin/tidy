//! The volume a path is on: capacity and what's free.

#[derive(Clone, Copy, Debug, Default)]
pub struct Space {
    pub total: u64,
    pub free: u64,
}

#[cfg(unix)]
pub fn space(path: &std::path::Path) -> Option<Space> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let bs = s.f_bsize as u64;
    Some(Space { total: s.f_blocks as u64 * bs, free: s.f_bavail as u64 * bs })
}

#[cfg(windows)]
pub fn space(path: &std::path::Path) -> Option<Space> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut free, mut total, mut _all_free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut _all_free) };
    (ok != 0).then_some(Space { total, free })
}

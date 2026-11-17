//! The volume a path is on: capacity and what's free.

#[derive(Clone, Copy, Debug, Default)]
pub struct Space {
    pub total: u64,
    pub free: u64,
}

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

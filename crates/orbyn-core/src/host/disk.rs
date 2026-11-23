//! Free space on the volume that holds a path.

/// Bytes available to this user on the volume that holds `path`, or `None` when the volume
/// cannot be read.
#[cfg(unix)]
pub fn free_bytes(path: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: `path` is a live null-terminated string and `stats` is a live place for one `statvfs`; the call writes only
    // that place, and on a nonzero return nothing is read.
    let status = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if status != 0 {
        return None;
    }
    // SAFETY: the call returned zero, so it initialized every field of `stats`, which is read here as plain data.
    let stats = unsafe { stats.assume_init() };
    Some((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

/// Bytes available to this user on the volume that holds `path`, or `None` when the volume
/// cannot be read.
#[cfg(windows)]
pub fn free_bytes(path: &std::path::Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return None;
    }
    wide.push(0);
    let mut free_to_caller = 0u64;
    let mut total_bytes = 0u64;
    let mut total_free = 0u64;
    // SAFETY: `wide` is a live null-terminated UTF-16 string, and each output pointer names a live `u64` this process owns;
    // the call either fails (and the outputs are not read) or writes all three.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total_bytes,
            &mut total_free,
        )
    };
    (ok != 0).then_some(free_to_caller)
}

/// A platform without a volume query reports no free space, which never blocks a start.
#[cfg(not(any(unix, windows)))]
pub fn free_bytes(_path: &std::path::Path) -> Option<u64> {
    None
}

/// `bytes` in gibibytes, the unit the free-space refusal speaks in.
pub fn gib(bytes: u64) -> f64 {
    bytes as f64 / 1073741824.0
}

#[cfg(test)]
mod tests {
    use super::{free_bytes, gib};

    #[test]
    fn the_temp_folder_reports_free_space() {
        let free = free_bytes(&std::env::temp_dir());
        assert!(free.is_some(), "the temp folder's volume is readable");
        assert!(free.is_some_and(|bytes| bytes > 0));
    }

    #[test]
    fn gibibytes_are_binary_gigabytes() {
        assert_eq!(gib(2 * 1024 * 1024 * 1024), 2.0);
    }
}

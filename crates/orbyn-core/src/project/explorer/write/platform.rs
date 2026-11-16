//! Exclusive filesystem operations on checked parents. No overwrite fallback.

use super::Location;
use std::io;

#[cfg(unix)]
fn name(location: &Location) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(
        location
            .path
            .file_name()
            .ok_or(io::ErrorKind::InvalidInput)?
            .as_bytes(),
    )
    .map_err(|_| io::ErrorKind::InvalidInput.into())
}

#[cfg(unix)]
fn result(code: libc::c_int) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub(super) fn create_file(at: &Location) -> io::Result<()> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let name = name(at)?;
    // SAFETY: the parent descriptor and NUL-terminated name live through openat.
    // O_EXCL refuses every existing entry, including a link, without overwriting.
    let fd = unsafe {
        libc::openat(
            at.parent().as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o666,
        )
    };
    if fd == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful openat returned a new owned descriptor, transferred once.
    drop(unsafe { std::fs::File::from_raw_fd(fd) });
    Ok(())
}

#[cfg(unix)]
pub(super) fn create_directory(at: &Location) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let name = name(at)?;
    // SAFETY: borrowed live parent and owned NUL-terminated leaf name.
    result(unsafe { libc::mkdirat(at.parent().as_raw_fd(), name.as_ptr(), 0o777) })
}

#[cfg(unix)]
pub(super) fn delete(at: &Location, directory: bool) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let name = name(at)?;
    let flags = if directory { libc::AT_REMOVEDIR } else { 0 };
    // SAFETY: borrowed live parent and owned NUL-terminated leaf name. unlinkat
    // never follows the final link; AT_REMOVEDIR removes empty directories only.
    result(unsafe { libc::unlinkat(at.parent().as_raw_fd(), name.as_ptr(), flags) })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn move_entry(from: &Location, to: &Location) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let (source, dest) = (name(from)?, name(to)?);
    let (source_fd, dest_fd) = (from.parent().as_raw_fd(), to.parent().as_raw_fd());
    #[cfg(target_os = "macos")]
    // SAFETY: both descriptors and NUL-terminated leaf names live through the call.
    // RENAME_EXCL atomically refuses an occupied destination, including folders.
    let code = unsafe {
        libc::renameatx_np(
            source_fd,
            source.as_ptr(),
            dest_fd,
            dest.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(target_os = "linux")]
    // SAFETY: same descriptor/name ownership as above; no flags allow replacement.
    let code = unsafe {
        libc::renameat2(
            source_fd,
            source.as_ptr(),
            dest_fd,
            dest.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    result(code).map_err(unsupported)
}

/// A file system that cannot do an exclusive rename (an old kernel, FUSE, some network
/// shares) answers with one of a few errors. They become `Unsupported`, so the person sees
/// that the folder cannot move files safely instead of a raw system error, and nothing falls
/// back to a rename that could overwrite.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(in crate::project::explorer) fn unsupported(error: io::Error) -> io::Error {
    let code = error.raw_os_error();
    if code
        .is_some_and(|code| code == libc::ENOSYS || code == libc::ENOTSUP || code == libc::EINVAL)
    {
        return io::ErrorKind::Unsupported.into();
    }
    error
}

#[cfg(windows)]
pub(super) fn create_file(at: &Location) -> io::Result<()> {
    drop(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&at.path)?,
    );
    Ok(())
}

#[cfg(windows)]
pub(super) fn create_directory(at: &Location) -> io::Result<()> {
    std::fs::create_dir(&at.path)
}

#[cfg(windows)]
pub(super) fn delete(at: &Location, directory: bool) -> io::Result<()> {
    if directory {
        std::fs::remove_dir(&at.path)
    } else {
        std::fs::remove_file(&at.path)
    }
}

#[cfg(windows)]
pub(super) fn move_entry(from: &Location, to: &Location) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let source: Vec<u16> = from.path.as_os_str().encode_wide().chain(Some(0)).collect();
    let dest: Vec<u16> = to.path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both UTF-16 buffers are NUL-terminated and remain alive throughout.
    // Zero flags refuses an existing destination and cross-volume copy fallback.
    if unsafe { MoveFileExW(source.as_ptr(), dest.as_ptr(), 0) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub(super) fn move_entry(_: &Location, _: &Location) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

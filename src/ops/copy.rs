//! Copying and moving bytes: recursive copy with progress, `copy_file_range`, cross-device moves.

use super::*;

pub(super) fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    copy_tracked(src, dst, &Ctl::detached())
}

pub(super) fn copy_tracked(src: &Path, dst: &Path, ctl: &Ctl) -> io::Result<()> {
    ctl.checkpoint()?;
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        symlink(fs::read_link(src)?, dst)
    } else if meta.is_dir() {
        // Refuse to copy a folder into itself, which would recurse forever.
        if dst.starts_with(src) {
            return Err(io::Error::other("cannot copy a folder into itself"));
        }
        fs::create_dir(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tracked(&entry.path(), &dst.join(entry.file_name()), ctl)?;
        }
        fs::set_permissions(dst, meta.permissions())
    } else {
        copy_file(src, dst, &meta, ctl)
    }
}

/// Copy one file in chunks so progress, pause and cancel work. The kernel does the copying when it can
/// (`copy_file_range`, which also shares blocks on filesystems that support it), else read and write.
pub(super) fn copy_file(src: &Path, dst: &Path, meta: &fs::Metadata, ctl: &Ctl) -> io::Result<()> {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    const CHUNK: usize = 8 << 20;
    let mut input = fs::File::open(src)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    let mut kernel = true;
    let mut buf = Vec::new();
    loop {
        ctl.checkpoint()?;
        let n = if kernel {
            // SAFETY: both descriptors are open files owned by this function; null offsets use and advance each file position.
            let r = unsafe {
                libc::copy_file_range(
                    input.as_raw_fd(),
                    std::ptr::null_mut(),
                    output.as_raw_fd(),
                    std::ptr::null_mut(),
                    CHUNK,
                    0,
                )
            };
            if r >= 0 {
                r as usize
            } else {
                // Not supported here (other filesystem on an old kernel, special file...): read and write instead.
                kernel = false;
                continue;
            }
        } else {
            buf.resize(1 << 20, 0);
            let n = input.read(&mut buf)?;
            output.write_all(&buf[..n])?;
            n
        };
        if n == 0 {
            break;
        }
        ctl.add_bytes(n as u64);
    }
    output.set_permissions(meta.permissions())
}

pub(super) fn remove_any(p: &Path) -> io::Result<()> {
    if fs::symlink_metadata(p)?.is_dir() {
        fs::remove_dir_all(p)
    } else {
        fs::remove_file(p)
    }
}

/// Rename, falling back to copy + delete across filesystems.
pub(super) fn move_any(src: &Path, dst: &Path) -> io::Result<()> {
    move_tracked(src, dst, &Ctl::detached())
}

pub(super) fn move_tracked(src: &Path, dst: &Path, ctl: &Ctl) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(18) => {
            copy_tracked(src, dst, ctl)?;
            remove_any(src)
        }
        Err(e) => Err(e),
    }
}

/// Total size of the regular files under `p`.
pub(super) fn tree_bytes(p: &Path) -> u64 {
    walkdir::WalkDir::new(p)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

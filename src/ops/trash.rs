//! The trash: the freedesktop home trash for local files, the server's own trash for remotes, and the undo helpers
//! that depend on which of the two a path is on.

use super::*;

// ---- trash -----------------------------------------------------------------------------

pub(super) fn trash_dirs() -> (PathBuf, PathBuf) {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local/share"))
        .join("Trash");
    (base.join("files"), base.join("info"))
}

pub fn percent_encode_path(p: &str) -> String {
    let mut out = String::new();
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub(super) fn deletion_date() -> String {
    Command::new("date")
        .arg("+%Y-%m-%dT%H:%M:%S")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

pub(super) fn trash_one(orig: &str, date: &str) -> io::Result<Step> {
    let (files, infos) = trash_dirs();
    fs::create_dir_all(&files)?;
    fs::create_dir_all(&infos)?;
    let dest = unique_dest(&files, &base_name(orig));
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let info = infos.join(format!("{name}.trashinfo"));
    fs::write(
        &info,
        format!(
            "[Trash Info]\nPath={}\nDeletionDate={date}\n",
            percent_encode_path(orig)
        ),
    )?;
    if let Err(e) = move_any(Path::new(orig), &dest) {
        let _ = fs::remove_file(&info);
        return Err(e);
    }
    Ok(Step::Trashed {
        orig: orig.to_owned(),
        stored: dest.to_string_lossy().into_owned(),
        info: info.to_string_lossy().into_owned(),
    })
}

pub(super) fn restore_one(orig: &str, stored: &str, info: &str) -> io::Result<()> {
    if fs::symlink_metadata(orig).is_ok() {
        return Err(io::Error::other(format!("{orig} already exists")));
    }
    move_any(Path::new(stored), Path::new(orig))?;
    let _ = fs::remove_file(info);
    Ok(())
}

/// Trash on the server itself, for a file on a remote. Moving it into the local trash instead would
/// download the whole file and then delete it on the server. Only gvfs's Google Drive has a trash;
/// SMB, SFTP, FTP, WebDAV and rclone mounts fail with `Unsupported`.
pub(super) fn server_trash(path: &str) -> io::Result<()> {
    let root = crate::listing::remote_root(path).unwrap_or_default();
    if !root
        .rsplit('/')
        .next()
        .unwrap_or("")
        .starts_with("google-drive:")
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this server has no trash",
        ));
    }
    let out = Command::new("gio").arg("trash").arg(path).output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    }
}

/// Whether replacing or trashing at `path` would delete for good: it is on a remote with no trash.
pub fn deletes_for_good(path: &str) -> bool {
    crate::listing::remote_root(path).is_some_and(|root| {
        !root
            .rsplit('/')
            .next()
            .unwrap_or("")
            .starts_with("google-drive:")
    })
}

/// Make room for a replacement at `path`. Locally the old item goes to the trash (an undo step); on a remote
/// it goes to the server's trash, or is deleted for good where there is none (the clash dialog said so).
pub(super) fn clear_for_replace(path: &str) -> io::Result<Option<Step>> {
    if !crate::listing::is_remote(path) {
        return trash_one(path, &deletion_date()).map(Some);
    }
    match server_trash(path) {
        Err(e) if e.kind() == io::ErrorKind::Unsupported => {
            remove_any(Path::new(path)).map(|()| None)
        }
        other => other.map(|()| None),
    }
}

/// Undo something this app made at `path`. Locally it goes to the trash. On a remote, a copy or a link is
/// removed directly (the original is still there, and the local trash would download it), and a new file or
/// folder only while it is still empty, since there may be no trash to get it back from.
pub(super) fn unmake(path: &str, made_empty: bool) -> io::Result<()> {
    if !crate::listing::is_remote(path) {
        return trash_one(path, &deletion_date()).map(|_| ());
    }
    if made_empty {
        let p = Path::new(path);
        let empty = if p.is_dir() {
            fs::read_dir(p)?.next().is_none()
        } else {
            fs::symlink_metadata(p)?.len() == 0
        };
        if !empty && server_trash(path).is_err() {
            return Err(io::Error::other(format!(
                "{} has changed since it was made and this server has no trash; delete it yourself",
                base_name(path)
            )));
        }
        if !empty {
            return Ok(());
        }
    }
    remove_any(Path::new(path))
}

pub fn trash(paths: &[String]) -> Outcome {
    let mut out = Outcome::new("Trashed");
    let date = deletion_date();
    let mut steps = Vec::new();
    for p in paths {
        if crate::listing::is_remote(p) {
            match server_trash(p) {
                Ok(()) => out.ok += 1,
                Err(e) if e.kind() == io::ErrorKind::Unsupported => out.no_trash.push(p.clone()),
                Err(e) => out.errors.push(format!("{}: {e}", base_name(p))),
            }
            continue;
        }
        match trash_one(p, &date) {
            Ok(step) => {
                steps.push(step);
                out.ok += 1;
            }
            Err(e) => out.errors.push(format!("{}: {e}", base_name(p))),
        }
    }
    out.undo = !steps.is_empty();
    record(Action {
        label: out.label.clone(),
        steps,
    });
    out
}

/// How many items are in the trash (the home trash, where `trash` puts things).
pub fn trash_count() -> usize {
    fs::read_dir(trash_dirs().0).map_or(0, |d| d.count())
}

/// Delete everything in the home trash for good. Not undoable, and the undo history that pointed into the
/// trash goes too, so undo moves on to the next thing it can still do instead of failing on a missing file.
pub fn empty_trash() -> Outcome {
    let mut out = Outcome::new("Emptied the trash of");
    let (files, infos) = trash_dirs();
    if let Ok(entries) = fs::read_dir(&files) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            match remove_any(&e.path()) {
                Ok(()) => {
                    out.ok += 1;
                    let _ = fs::remove_file(infos.join(format!("{name}.trashinfo")));
                }
                Err(err) => out.errors.push(format!("{name}: {err}")),
            }
        }
    }
    // Records whose item is gone (written by other programs, or left behind by a crash).
    if let Ok(entries) = fs::read_dir(&infos) {
        for e in entries.flatten() {
            let stem = e
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".trashinfo")
                .to_owned();
            if fs::symlink_metadata(files.join(stem)).is_err() {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    if let Some(trash) = files.parent() {
        let _ = fs::remove_file(trash.join("directorysizes"));
    }
    let mut j = JOURNAL.locked();
    let keep = |a: &Action| !a.steps.iter().any(|s| matches!(s, Step::Trashed { .. }));
    j.undo.retain(keep);
    j.redo.retain(keep);
    out
}

use super::*;
use crate::testutil::Sandbox;

fn p(sb: &Sandbox, rel: &str) -> String {
    sb.path(rel)
}

#[test]
fn trash_then_undo_then_redo() {
    let sb = Sandbox::new("trash");
    sb.touch("a.txt", "A");
    let out = trash(&[p(&sb, "a.txt")]);
    assert_eq!((out.ok, out.errors.len()), (1, 0), "{:?}", out.errors);
    assert!(!Path::new(&p(&sb, "a.txt")).exists());
    // Freedesktop layout: the file and its .trashinfo record.
    let (files, infos) = trash_dirs();
    assert!(files.join("a.txt").exists());
    let info = fs::read_to_string(infos.join("a.txt.trashinfo")).unwrap();
    assert!(
        info.contains("[Trash Info]")
            && info.contains(&format!("Path={}", percent_encode_path(&p(&sb, "a.txt"))))
    );

    let undo = step_history(false);
    assert_eq!(undo.ok, 1, "{:?}", undo.errors);
    assert_eq!(fs::read_to_string(p(&sb, "a.txt")).unwrap(), "A");
    assert!(!infos.join("a.txt.trashinfo").exists());

    let redo = step_history(true);
    assert_eq!(redo.ok, 1, "{:?}", redo.errors);
    assert!(!Path::new(&p(&sb, "a.txt")).exists());
}

#[test]
fn emptying_the_trash_removes_everything_in_it() {
    let sb = Sandbox::new("empty-trash");
    assert_eq!(
        (empty_trash().ok, empty_trash().errors.len()),
        (0, 0),
        "an empty trash is fine"
    );
    sb.touch("a.txt", "A");
    sb.touch("b.txt", "B");
    sb.touch("d/inner.txt", "I");
    trash(&[p(&sb, "a.txt"), p(&sb, "b.txt"), p(&sb, "d")]);
    assert_eq!(trash_count(), 3);
    let (files, infos) = trash_dirs();
    fs::write(files.parent().unwrap().join("directorysizes"), "").unwrap();
    fs::write(infos.join("orphan.trashinfo"), "[Trash Info]\n").unwrap();
    let out = empty_trash();
    assert_eq!((out.ok, out.errors.len()), (3, 0), "{:?}", out.errors);
    assert_eq!(trash_count(), 0);
    assert_eq!(
        fs::read_dir(&infos).unwrap().count(),
        0,
        "records go too, orphans included"
    );
    assert!(
        files.is_dir() && infos.is_dir(),
        "the trash folders themselves stay"
    );
    assert!(!files.parent().unwrap().join("directorysizes").exists());
}

#[test]
fn undo_after_emptying_skips_what_was_in_the_trash() {
    let sb = Sandbox::new("empty-trash-undo");
    while step_history(false).errors != ["Nothing to undo"] {}
    sb.touch("a.txt", "A");
    sb.touch("b.txt", "B");
    trash(&[p(&sb, "a.txt")]);
    assert_eq!(rename(&p(&sb, "b.txt"), "c.txt").ok, 1);
    empty_trash();
    let undo = step_history(false);
    assert_eq!(
        (undo.ok, undo.errors.len()),
        (1, 0),
        "the rename is undone: {:?}",
        undo.errors
    );
    assert!(Path::new(&p(&sb, "b.txt")).exists());
    assert_eq!(
        step_history(false).errors,
        ["Nothing to undo"],
        "the trashing is gone from history"
    );
}

#[test]
fn trashing_two_files_with_the_same_name_keeps_both() {
    let sb = Sandbox::new("trash-twins");
    sb.touch("x/a.txt", "from x");
    sb.touch("y/a.txt", "from y");
    let out = trash(&[p(&sb, "x/a.txt"), p(&sb, "y/a.txt")]);
    assert_eq!(out.ok, 2, "{:?}", out.errors);
    let (files, infos) = trash_dirs();
    let mut stored: Vec<String> = fs::read_dir(&files)
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    stored.sort();
    assert_eq!(stored, ["from x", "from y"]);
    assert_eq!(fs::read_dir(&infos).unwrap().count(), 2);
    // One undo puts each back in its own folder.
    assert_eq!(step_history(false).ok, 2);
    assert_eq!(fs::read_to_string(p(&sb, "x/a.txt")).unwrap(), "from x");
    assert_eq!(fs::read_to_string(p(&sb, "y/a.txt")).unwrap(), "from y");
}

#[test]
fn trashing_a_folder_and_undo_brings_its_contents_back() {
    let sb = Sandbox::new("trash-dir");
    sb.touch("d/sub/deep.txt", "deep");
    sb.touch("d/top.txt", "top");
    assert_eq!(trash(&[p(&sb, "d")]).ok, 1);
    assert!(!Path::new(&p(&sb, "d")).exists());
    assert_eq!(step_history(false).ok, 1);
    assert_eq!(
        fs::read_to_string(p(&sb, "d/sub/deep.txt")).unwrap(),
        "deep"
    );
    assert_eq!(fs::read_to_string(p(&sb, "d/top.txt")).unwrap(), "top");
}

#[test]
fn a_missing_file_does_not_stop_the_rest_of_the_trash() {
    let sb = Sandbox::new("trash-partial");
    sb.touch("a.txt", "A");
    let out = trash(&[p(&sb, "gone.txt"), p(&sb, "a.txt")]);
    assert_eq!((out.ok, out.errors.len()), (1, 1));
    assert!(out.errors[0].starts_with("gone.txt"), "{:?}", out.errors);
    // No .trashinfo is left behind for the one that failed.
    let (_, infos) = trash_dirs();
    assert!(!infos.join("gone.txt.trashinfo").exists());
    assert_eq!(step_history(false).ok, 1);
    assert!(Path::new(&p(&sb, "a.txt")).exists());
}

#[test]
fn moving_across_drives_copies_then_removes() {
    use std::os::unix::fs::MetadataExt;
    let sb = Sandbox::new("xdev");
    // Needs a second filesystem; /dev/shm is a separate tmpfs on most systems.
    let other = Path::new("/dev/shm").join(format!("cfm-xdev-{}", std::process::id()));
    if fs::create_dir_all(&other).is_err()
        || fs::metadata(&other).unwrap().dev() == fs::metadata(&sb.root).unwrap().dev()
    {
        eprintln!("skipped: no second filesystem at /dev/shm");
        return;
    }
    sb.touch("d/inner.txt", "inner");
    sb.touch("f.txt", "file");
    move_any(Path::new(&p(&sb, "d")), &other.join("d")).unwrap();
    move_any(Path::new(&p(&sb, "f.txt")), &other.join("f.txt")).unwrap();
    let moved = (
        fs::read_to_string(other.join("d/inner.txt")).ok(),
        fs::read_to_string(other.join("f.txt")).ok(),
    );
    let _ = fs::remove_dir_all(&other);
    assert_eq!(moved, (Some("inner".into()), Some("file".into())));
    assert!(!Path::new(&p(&sb, "d")).exists() && !Path::new(&p(&sb, "f.txt")).exists());
}

#[test]
fn a_remote_file_never_lands_in_the_local_trash() {
    let mut sb = Sandbox::new("trash-remote");
    // Anything under the gvfs folder counts as remote (`listing::remote_root`).
    sb.set_env("XDG_RUNTIME_DIR", sb.root.join("run"));
    let share = "run/gvfs/smb-share:server=nas,share=media";
    sb.touch(&format!("{share}/a.txt"), "A");
    assert!(crate::listing::is_remote(&p(
        &sb,
        &format!("{share}/a.txt")
    )));
    let out = trash(&[p(&sb, &format!("{share}/a.txt"))]);
    let (files, _) = trash_dirs();
    assert!(!files.join("a.txt").exists());
    // An SMB share has no trash: the file stays, and the UI is told so it can offer to delete it.
    assert_eq!(out.no_trash, [p(&sb, &format!("{share}/a.txt"))]);
    assert!(Path::new(&p(&sb, &format!("{share}/a.txt"))).exists());
    assert_eq!((out.ok, out.undo), (0, false));
}

#[test]
fn replacing_on_a_remote_never_downloads_the_old_file() {
    let mut sb = Sandbox::new("replace-remote");
    sb.set_env("XDG_RUNTIME_DIR", sb.root.join("run"));
    let share = "run/gvfs/smb-share:server=nas,share=media";
    sb.touch("src/a.txt", "new");
    sb.touch(&format!("{share}/a.txt"), "old");
    let dst = p(&sb, share);
    assert!(deletes_for_good(&dst));
    let ctl = Ctl::register("Copied", &dst, 1);
    let out = paste_tracked(&ctl, &[p(&sb, "src/a.txt")], &dst, false, Policy::Replace);
    assert_eq!(out.ok, 1, "{:?}", out.errors);
    assert_eq!(fs::read_to_string(format!("{dst}/a.txt")).unwrap(), "new");
    let (files, _) = trash_dirs();
    assert!(
        !files.join("a.txt").exists(),
        "the old file must not come down into the local trash"
    );
    // Undo removes the copy, on the server, without downloading it either.
    assert_eq!(step_history(false).ok, 1);
    assert!(!Path::new(&format!("{dst}/a.txt")).exists());
    assert!(!files.join("a.txt").exists());
    assert!(
        !deletes_for_good(&p(&sb, "src")),
        "local folders have a trash"
    );
}

#[test]
fn undoing_a_new_remote_file_spares_one_that_was_written_to() {
    let mut sb = Sandbox::new("unmake-remote");
    sb.set_env("XDG_RUNTIME_DIR", sb.root.join("run"));
    let share = p(&sb, "run/gvfs/sftp:host=nas");
    fs::create_dir_all(&share).unwrap();
    assert_eq!(create(&share, "empty.txt", false).ok, 1);
    assert_eq!(step_history(false).ok, 1);
    assert!(
        !Path::new(&format!("{share}/empty.txt")).exists(),
        "an untouched new file is removed"
    );
    assert_eq!(create(&share, "notes.txt", false).ok, 1);
    fs::write(format!("{share}/notes.txt"), "work in progress").unwrap();
    let undo = step_history(false);
    assert_eq!(undo.ok, 0);
    assert!(undo.errors[0].contains("no trash"), "{:?}", undo.errors);
    assert_eq!(
        fs::read_to_string(format!("{share}/notes.txt")).unwrap(),
        "work in progress"
    );
}

#[test]
fn local_and_remote_together_trash_the_local_part_only() {
    let mut sb = Sandbox::new("trash-mixed");
    sb.set_env("XDG_RUNTIME_DIR", sb.root.join("run"));
    let remote = "run/gvfs/sftp:host=nas/b.txt";
    sb.touch(remote, "B");
    sb.touch("a.txt", "A");
    let out = trash(&[p(&sb, "a.txt"), p(&sb, remote)]);
    assert_eq!((out.ok, out.undo), (1, true));
    assert_eq!(out.no_trash, [p(&sb, remote)]);
    assert!(!Path::new(&p(&sb, "a.txt")).exists());
    // Undo brings back the local file; the remote one was never touched.
    assert_eq!(step_history(false).ok, 1);
    assert!(Path::new(&p(&sb, "a.txt")).exists() && Path::new(&p(&sb, remote)).exists());
}

#[test]
fn undo_refuses_to_overwrite_a_new_file() {
    let sb = Sandbox::new("undo-clash");
    sb.touch("a.txt", "old");
    trash(&[p(&sb, "a.txt")]);
    sb.touch("a.txt", "new");
    let undo = step_history(false);
    assert_eq!(undo.ok, 0);
    assert!(undo.errors[0].contains("already exists"));
    assert_eq!(fs::read_to_string(p(&sb, "a.txt")).unwrap(), "new");
}

#[test]
fn cut_paste_moves_and_undo_moves_back() {
    let sb = Sandbox::new("cutpaste");
    sb.touch("src/a.txt", "A");
    sb.touch("dst/", "");
    let out = paste(&[p(&sb, "src/a.txt")], &p(&sb, "dst"), true);
    assert_eq!(out.ok, 1);
    assert_eq!(out.paths, [p(&sb, "dst/a.txt")]);
    assert!(!Path::new(&p(&sb, "src/a.txt")).exists());
    assert_eq!(step_history(false).ok, 1);
    assert_eq!(fs::read_to_string(p(&sb, "src/a.txt")).unwrap(), "A");
}

#[test]
fn copy_paste_into_same_folder_makes_a_copy_and_undo_trashes_it() {
    let sb = Sandbox::new("copy");
    sb.touch("a.txt", "A");
    let out = paste(&[p(&sb, "a.txt")], &p(&sb, ""), false);
    assert_eq!(out.paths, [p(&sb, "a (copy).txt")]);
    assert_eq!(fs::read_to_string(p(&sb, "a (copy).txt")).unwrap(), "A");
    step_history(false);
    assert!(!Path::new(&p(&sb, "a (copy).txt")).exists());
    assert!(Path::new(&p(&sb, "a.txt")).exists());
}

#[test]
fn moving_into_the_same_folder_is_a_no_op() {
    let sb = Sandbox::new("noop");
    sb.touch("a.txt", "A");
    let out = paste(&[p(&sb, "a.txt")], &p(&sb, ""), true);
    assert_eq!(out.ok, 0);
    assert!(out.errors.is_empty());
}

#[test]
fn rename_create_and_delete() {
    let sb = Sandbox::new("rename");
    sb.touch("a.txt", "A");
    assert_eq!(rename(&p(&sb, "a.txt"), "b.txt").ok, 1);
    assert!(Path::new(&p(&sb, "b.txt")).exists());
    // Names with slashes, empty names and existing targets are rejected.
    assert!(!rename(&p(&sb, "b.txt"), "x/y").errors.is_empty());
    assert!(!rename(&p(&sb, "b.txt"), "").errors.is_empty());
    sb.touch("c.txt", "");
    assert!(rename(&p(&sb, "b.txt"), "c.txt").errors[0].contains("already exists"));

    assert_eq!(create(&p(&sb, ""), "new.txt", false).ok, 1);
    assert_eq!(create(&p(&sb, ""), "newdir", true).ok, 1);
    assert!(Path::new(&p(&sb, "newdir")).is_dir());
    assert!(
        !create(&p(&sb, ""), "new.txt", false).errors.is_empty(),
        "must not clobber"
    );

    let gone = delete(&[p(&sb, "newdir"), p(&sb, "new.txt")]);
    assert_eq!(gone.ok, 2);
    assert!(!Path::new(&p(&sb, "new.txt")).exists());
    // Permanent deletion is not journaled: the next undo reverts something older.
    assert!(delete(&[p(&sb, "missing")]).errors.len() == 1);
}

#[test]
fn batch_rename_is_one_undo_step() {
    let sb = Sandbox::new("batch");
    for n in ["a", "b", "c"] {
        sb.touch(n, n);
    }
    let pairs: Vec<(String, String)> = ["a", "b", "c"]
        .iter()
        .map(|n| (p(&sb, n), p(&sb, &format!("{n}.bak"))))
        .collect();
    assert_eq!(rename_many(&pairs).ok, 3);
    let undo = step_history(false);
    assert_eq!(undo.ok, 3, "{:?}", undo.errors);
    assert!(
        ["a", "b", "c"]
            .iter()
            .all(|n| Path::new(&p(&sb, n)).exists())
    );
}

#[test]
fn empty_history_reports_nothing_to_undo() {
    let _sb = Sandbox::new("empty-history");
    // Drain whatever earlier tests left behind (their folders are gone, so some undos fail: keep going).
    while step_history(false).errors != ["Nothing to undo"] {}
    let out = step_history(false);
    assert_eq!(out.ok, 0);
    assert_eq!(out.errors, ["Nothing to undo"]);
}

#[test]
fn name_clashes_replace_skip_or_keep_both() {
    let sb = Sandbox::new("clash");
    sb.touch("src/a.txt", "new");
    sb.touch("dst/a.txt", "old");
    let (src, dst) = (sb.path("src/a.txt"), sb.path("dst"));
    assert_eq!(conflicts(std::slice::from_ref(&src), &dst), ["a.txt"]);
    let run = |policy| {
        let ctl = Ctl::register("Copied", &dst, 1);
        paste_tracked(&ctl, std::slice::from_ref(&src), &dst, false, policy)
    };
    let out = run(Policy::Skip);
    assert_eq!((out.ok, out.skipped), (0, 1));
    assert_eq!(fs::read_to_string(sb.path("dst/a.txt")).unwrap(), "old");
    assert_eq!(run(Policy::Both).paths, [sb.path("dst/a (copy).txt")]);
    let out = run(Policy::Replace);
    assert_eq!(out.ok, 1);
    assert_eq!(fs::read_to_string(sb.path("dst/a.txt")).unwrap(), "new");
    // Undo puts the old file back (it went to the trash, not away for good).
    let undone = step_history(false);
    assert!(undone.errors.is_empty(), "{:?}", undone.errors);
    assert_eq!(fs::read_to_string(sb.path("dst/a.txt")).unwrap(), "old");
}

#[test]
fn a_cancelled_copy_leaves_no_partial_file() {
    let sb = Sandbox::new("cancel");
    sb.touch("a.txt", "data");
    let ctl = Ctl::register("Copied", &sb.path("d"), 1);
    fs::create_dir(sb.path("d")).unwrap();
    crate::transfer::cancel_ctl(&ctl);
    let out = paste_tracked(
        &ctl,
        &[sb.path("a.txt")],
        &sb.path("d"),
        false,
        Policy::Both,
    );
    assert!(out.cancelled);
    assert!(!Path::new(&sb.path("d/a.txt")).exists());
}

#[test]
fn copying_a_folder_into_itself_fails_cleanly() {
    let sb = Sandbox::new("self-copy");
    sb.touch("d/f", "x");
    let out = paste(&[p(&sb, "d")], &p(&sb, "d"), false);
    assert_eq!(out.ok, 0);
    assert!(!out.errors.is_empty());
}

#[test]
fn a_paste_where_every_item_fails_ends_failed_and_a_mixed_one_ends_done() {
    use crate::transfer::State;
    let sb = Sandbox::new("failed-job");
    sb.touch("d/f", "x");
    sb.touch("ok.txt", "y");
    fs::create_dir(sb.path("dest")).unwrap();

    let ctl = Ctl::register("Copied", &sb.path("d"), 1);
    let out = paste_tracked(&ctl, &[sb.path("d")], &sb.path("d"), false, Policy::Both);
    assert_eq!(out.ok, 0);
    assert_eq!(ctl.state(), State::Failed);

    let ctl = Ctl::register("Copied", &sb.path("dest"), 2);
    let both = [sb.path("ok.txt"), sb.path("missing.txt")];
    let out = paste_tracked(&ctl, &both, &sb.path("dest"), false, Policy::Both);
    assert_eq!((out.ok, out.errors.len()), (1, 1));
    assert_eq!(ctl.state(), State::Done);
}

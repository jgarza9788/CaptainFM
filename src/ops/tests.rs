use super::*;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cfm-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn linking_makes_undoable_symlinks() {
    let sb = crate::testutil::Sandbox::new("ops-link");
    sb.touch("src/a.txt", "A");
    sb.touch("dst/", "");
    let out = link(&[sb.path("src/a.txt")], &sb.path("dst"));
    assert_eq!(out.ok, 1, "{:?}", out.errors);
    let l = sb.path("dst/a.txt");
    assert_eq!(fs::read_link(&l).unwrap(), Path::new(&sb.path("src/a.txt")));
    assert_eq!(fs::read_to_string(&l).unwrap(), "A");
    // A second link gets a fresh name.
    link(&[sb.path("src/a.txt")], &sb.path("dst"));
    assert!(Path::new(&sb.path("dst/a (copy).txt")).exists());
}

#[test]
fn download_names_come_from_the_url() {
    assert_eq!(
        url_file_name("https://example.com/a/b%20c.png?x=1#top"),
        "b c.png"
    );
    assert_eq!(url_file_name("https://example.com/"), "download");
    assert_eq!(url_file_name("https://example.com"), "download");
    assert_eq!(url_file_name("http://h/dir/.."), "download");
}

#[test]
fn archive_stems() {
    assert_eq!(archive_stem("a.tar.gz"), "a");
    assert_eq!(archive_stem("photos.ZIP"), "photos");
    assert_eq!(archive_stem("v1.2.zip"), "v1.2");
    assert_eq!(archive_stem(".zip"), ".zip");
}

#[test]
fn extract_here_and_into_folder() {
    let d = tmp("extract");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/f.txt"), "hi").unwrap();
    let zip = d.join("pack.zip").to_string_lossy().into_owned();
    let made = Command::new("bsdtar")
        .args(["--format", "zip", "-cf", &zip, "-C"])
        .arg(d.join("src"))
        .arg("f.txt")
        .status();
    if made.is_err() {
        return;
    }
    let o = extract(std::slice::from_ref(&zip), true);
    assert_eq!(o.errors.len(), 0, "{:?}", o.errors);
    assert!(d.join("pack/f.txt").exists());
    let o = extract(std::slice::from_ref(&zip), true);
    assert!(d.join("pack (copy)/f.txt").exists(), "{:?}", o.errors);
    fs::remove_file(d.join("pack/f.txt")).unwrap();
    fs::remove_dir(d.join("pack")).unwrap();
    extract(&[zip], false);
    assert!(d.join("f.txt").exists());
    let bad = d.join("src/f.txt").to_string_lossy().into_owned();
    assert_eq!(extract(&[bad], true).errors.len(), 1);
    assert!(!d.join("src/f (copy)").exists() && !d.join("src/f").exists());
}

#[test]
fn unique_names() {
    let d = tmp("unique");
    fs::write(d.join("a.txt"), "x").unwrap();
    assert_eq!(unique_dest(&d, "a.txt"), d.join("a (copy).txt"));
    fs::write(d.join("a (copy).txt"), "x").unwrap();
    assert_eq!(unique_dest(&d, "a.txt"), d.join("a (copy 2).txt"));
    assert_eq!(unique_dest(&d, "new"), d.join("new"));
}

#[test]
fn copy_dir_and_refuse_self_copy() {
    let d = tmp("copy");
    fs::create_dir_all(d.join("src/sub")).unwrap();
    fs::write(d.join("src/sub/f"), "hi").unwrap();
    copy_recursive(&d.join("src"), &d.join("dst")).unwrap();
    assert_eq!(fs::read_to_string(d.join("dst/sub/f")).unwrap(), "hi");
    assert!(copy_recursive(&d.join("src"), &d.join("src/inner")).is_err());
}

#[test]
fn rename_many_handles_shift() {
    let d = tmp("shift");
    for n in ["a", "b"] {
        fs::write(d.join(n), n).unwrap();
    }
    let p = |n: &str| d.join(n).to_string_lossy().into_owned();
    let out = rename_many(&[(p("a"), p("b")), (p("b"), p("c"))]);
    assert_eq!(out.ok, 2, "{:?}", out.errors);
    assert_eq!(fs::read_to_string(d.join("c")).unwrap(), "b");
}

#[test]
fn percent_encoding() {
    assert_eq!(percent_encode_path("/a b/é"), "/a%20b/%C3%A9");
}

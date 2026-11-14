use super::*;
use crate::dbus::percent_decode;

#[test]
fn drag_payload_carries_uris_and_plain_paths() {
    let v = drag_payload(&["/a b/c#1".to_owned(), "/d".to_owned()]);
    assert_eq!(v["uriList"], "file:///a%20b/c%231\r\nfile:///d\r\n");
    assert_eq!(v["plain"], "/a b/c#1\n/d");
}

#[test]
fn drops_accept_local_urls_and_ignore_remote_ones() {
    let urls = [
        "file:///a%20b".to_owned(),
        "file://localhost/x".to_owned(),
        "https://example.com/f".to_owned(),
        "smb://nas/s".to_owned(),
    ];
    assert_eq!(drop_paths(&urls, ""), ["/a b", "/x"]);
    assert!(drop_paths(&["https://example.com/f".to_owned()], "").is_empty());
    assert_eq!(drop_paths(&[], "/tmp\nnot a path\nfile:///tmp"), ["/tmp"]);
}

static LOOKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn hung_lookup(_: &str) -> bool {
    LOOKS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(400));
    false
}

fn quick_no(_: &str) -> bool {
    false
}

#[test]
fn a_remote_that_does_not_answer_is_not_waited_on_again() {
    let wait = Duration::from_millis(60);
    let root = "/test/slow-remote";
    let t = Instant::now();
    assert!(
        bounded_lookup_on(root, format!("{root}/a"), hung_lookup, wait),
        "no answer counts as yes"
    );
    assert!(t.elapsed() >= wait);
    // The second and third lookups there return at once, without asking the server.
    let t = Instant::now();
    assert!(bounded_lookup_on(
        root,
        format!("{root}/b"),
        hung_lookup,
        wait
    ));
    assert!(bounded_lookup_on(
        root,
        format!("{root}/c"),
        hung_lookup,
        wait
    ));
    assert!(t.elapsed() < wait);
    assert_eq!(LOOKS.load(std::sync::atomic::Ordering::SeqCst), 1);
    // Another remote still gets its real answer.
    assert!(!bounded_lookup_on(
        "/test/other-remote",
        "/test/other-remote/x".into(),
        quick_no,
        wait
    ));
}

#[test]
fn only_the_latest_preview_waits_while_one_is_read() {
    let q = LatestOnly::new();
    assert!(q.offer("a"), "the first offer starts the worker");
    assert_eq!(q.next(), Some("a"));
    // While "a" is being read, the cursor moves on: only the last one waits.
    assert!(!q.offer("b"));
    assert!(!q.offer("c"));
    assert_eq!(q.next(), Some("c"));
    assert_eq!(q.next(), None, "the worker stops");
    assert!(q.offer("d"), "and the next offer starts a new one");
}

#[test]
fn same_device_compares_filesystems() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_owned());
    assert!(same_device(std::slice::from_ref(&home), &home));
    assert!(!same_device(&[], &home));
    assert!(!same_device(&["/nonexistent/x".to_owned()], &home));
    assert!(!same_device(&[home], "/nonexistent"));
}

#[test]
fn web_drops_become_downloads() {
    assert_eq!(
        drop_urls(&["https://a/b.png".to_owned(), "file:///x".to_owned()], ""),
        ["https://a/b.png"]
    );
    assert_eq!(drop_urls(&[], "https://a/b.png\r\n"), ["https://a/b.png"]);
    assert!(drop_urls(&[], "see https://a/b.png for details").is_empty());
    assert!(drop_urls(&[], "plain words").is_empty());
}

#[test]
fn file_list_round_trip() {
    let body = format!("cut\n{}\n{}", file_url("/a b/c"), file_url("/d"));
    let (paths, cut) = parse_file_list(&body);
    assert!(cut);
    assert_eq!(paths, ["/a b/c", "/d"]);
}

#[test]
fn palette_ranking_modes() {
    let items: Vec<String> = ["Close tab", "New tab", "Next tab", "Open terminal"]
        .map(String::from)
        .to_vec();
    assert_eq!(rank_items(&items, "new tab", true), [1]);
    // A single term ranks word-start matches first ("New tab" and "Next tab" start with n/t words).
    let r = rank_items(&items, "t", true);
    assert_eq!(r.len(), 4);
    assert!(r[0] == 1 || r[0] == 3);
    assert_eq!(rank_items(&items, "re:^(new|next)", true), [1, 2]);
    assert_eq!(rank_items(&items, "/tab$/", true), [0, 1, 2]);
    assert!(rank_items(&items, "re:(", true).is_empty());
    // No title has "nwtb" as substrings, so it falls back to fuzzy (unless disabled).
    assert_eq!(rank_items(&items, "nwtb", true), [1]);
    assert!(rank_items(&items, "nwtb", false).is_empty());
}

#[test]
fn decode_bad_escape_is_literal() {
    assert_eq!(percent_decode("50%"), "50%");
    assert_eq!(percent_decode("%zz"), "%zz");
    // Multibyte text right after a % used to panic on a mid-character slice.
    assert_eq!(percent_decode("%aé"), "%aé");
    assert_eq!(percent_decode("%é"), "%é");
    assert_eq!(percent_decode("a%20b"), "a b");
}

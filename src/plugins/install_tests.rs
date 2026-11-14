use super::*;
use crate::testutil::Sandbox;

fn install(sb: &Sandbox, name: &str, manifest: &str, lua: &str) {
    sb.touch(
        &format!(".config/captainfm/plugins/{name}/plugin.json"),
        manifest,
    );
    sb.touch(&format!(".config/captainfm/plugins/{name}/main.lua"), lua);
}

#[test]
fn lists_installed_plugins_with_panels_and_actions() {
    let sb = Sandbox::new("plugins-list");
    install(
        &sb,
        "demo",
        r#"{"name":"Demo","panels":[{"id":"p","title":"Panel","qml":"P.qml","side":"bottom"}],
            "actions":[{"id":"go","title":"Go","keys":["<Space> p g"]}]}"#,
        "function go() end",
    );
    install(&sb, "broken", "{ not json", "");
    let listed = list();
    let mine: Vec<&Value> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["bundled"] == false)
        .collect();
    let arr = mine;
    assert_eq!(arr.len(), 1, "a plugin with an invalid manifest is skipped");
    assert_eq!(arr[0]["name"], "Demo");
    assert_eq!(arr[0]["panels"][0]["side"], "bottom");
    assert!(
        arr[0]["panels"][0]["qml"]
            .as_str()
            .unwrap()
            .starts_with("file://")
    );
    assert_eq!(arr[0]["actions"][0]["keys"][0], "<Space> p g");
}

#[test]
fn runs_an_installed_action() {
    let sb = Sandbox::new("plugins-run");
    install(
        &sb,
        "hello",
        r#"{"actions":[{"id":"hi","function":"greet"}]}"#,
        r#"function greet(ctx) cfm.notify("hi from " .. ctx.path); return 42 end"#,
    );
    let effects = run_action("hello", "hi", &json!({"path": "/tmp"}));
    assert_eq!(effects[0]["type"], "toast");
    assert_eq!(effects[0]["message"], "hi from /tmp");
    assert_eq!(effects[1]["value"], 42);
}

#[test]
fn bad_requests_become_error_toasts() {
    let sb = Sandbox::new("plugins-errors");
    install(
        &sb,
        "p",
        r#"{"actions":[{"id":"a"}]}"#,
        "function a() error('boom') end",
    );
    let kind = |v: Value| v[0]["kind"].as_str().unwrap().to_owned();
    assert_eq!(kind(run_action("../evil", "a", &json!({}))), "error");
    assert_eq!(kind(run_action("missing", "a", &json!({}))), "error");
    assert_eq!(kind(run_action("p", "nope", &json!({}))), "error");
    let failed = run_action("p", "a", &json!({}));
    assert_eq!(kind(failed.clone()), "error");
    assert!(failed[0]["message"].as_str().unwrap().contains("boom"));
}

// ---- bundled preview plugins -------------------------------------------------------------

#[cfg(any(
    feature = "plugin-torrent",
    feature = "plugin-calendar-contacts",
    feature = "plugin-system",
    feature = "plugin-documents"
))]
fn bundled(id: &str) -> String {
    BUNDLED.iter().find(|b| b.0 == id).unwrap().2[0]
        .1
        .to_owned()
}

/// Run `function` of a bundled plugin on `path`, plus any extra Lua appended to its script.
#[cfg(any(
    feature = "plugin-torrent",
    feature = "plugin-calendar-contacts",
    feature = "plugin-system",
    feature = "plugin-documents"
))]
fn preview_with(id: &str, extra: &str, function: &str, path: &Path) -> Option<Preview> {
    let ctx = json!({"path": path.to_string_lossy(), "name": path.file_name().unwrap().to_string_lossy(),
                     "ext": "", "size": std::fs::metadata(path).map_or(0, |m| m.len()), "mtime": 1});
    run_preview(&format!("{}\n{extra}", bundled(id)), id, function, &ctx).unwrap()
}

#[cfg(any(
    feature = "plugin-torrent",
    feature = "plugin-calendar-contacts",
    feature = "plugin-system",
    feature = "plugin-documents"
))]
fn text_of(p: Option<Preview>) -> String {
    match p {
        Some(Preview::Text { text, .. }) => text,
        _ => String::new(),
    }
}

#[test]
fn every_bundled_manifest_is_valid() {
    for (id, json, files) in BUNDLED {
        let m: Manifest = serde_json::from_str(json).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(!m.previews.is_empty(), "{id} has no previews");
        for spec in &m.previews {
            let file = if spec.file.is_empty() {
                "main.lua"
            } else {
                spec.file.as_str()
            };
            assert!(files.iter().any(|f| f.0 == file), "{id}: {file} missing");
            assert!(
                !spec.ext.is_empty() || !spec.magic.is_empty(),
                "{id}/{} matches nothing",
                spec.id
            );
        }
    }
}

#[test]
#[cfg(feature = "plugin-torrent")]
fn torrent_files_and_trackers() {
    let sb = Sandbox::new("plugin-torrent");
    let mut t = b"d8:announce21:http://tracker.test/a7:comment5:hello4:infod6:lengthi1048576e4:name8:disc.iso12:piece lengthi262144e6:pieces80:".to_vec();
    t.extend(std::iter::repeat_n(b'x', 80));
    t.extend_from_slice(b"ee");
    let file = sb.touch("a.torrent", "");
    std::fs::write(&file, &t).unwrap();
    let s = text_of(preview_with("preview-torrent", "", "preview", &file));
    assert!(s.contains("name: disc.iso"), "{s}");
    assert!(s.contains("1.0 MB"), "{s}");
    assert!(s.contains("pieces: 4 × 256.0 KB"), "{s}");
    assert!(s.contains("http://tracker.test/a"), "{s}");
    // Not a torrent, or hostile input: nothing, and no hang.
    for junk in [
        &b"not bencode"[..],
        &[b'l'; 100][..],
        b"99999999999999999999:x",
        b"5:ab",
    ] {
        std::fs::write(&file, junk).unwrap();
        assert!(preview_with("preview-torrent", "", "preview", &file).is_none());
    }
}

#[test]
#[cfg(feature = "plugin-calendar-contacts")]
fn calendars_list_events_with_times() {
    let sb = Sandbox::new("plugin-ics");
    let file = sb.touch("a.ics", "");
    std::fs::write(&file, "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Lunch\\, with Sam\nDTSTART:20261023T120000Z\nDTEND:20261023T130000Z\nLOCATION:Cafe\nDESCRIPTION:Bring the\n  notes\nEND:VEVENT\nBEGIN:VTODO\nSUMMARY:Pay rent\nDUE;VALUE=DATE:20261101\nEND:VTODO\nEND:VCALENDAR\n").unwrap();
    let s = text_of(preview_with(
        "preview-calendar-contacts",
        "",
        "calendar",
        &file,
    ));
    assert!(s.contains("2 entries"), "{s}");
    assert!(s.contains("• Lunch, with Sam"), "{s}");
    assert!(
        s.contains("when: 2026-10-23 12:00 UTC → 2026-10-23 13:00 UTC"),
        "{s}"
    );
    assert!(s.contains("where: Cafe"), "{s}");
    assert!(s.contains("Bring the notes"), "{s}");
    assert!(
        s.contains("☐ Pay rent") && s.contains("when: 2026-11-01"),
        "{s}"
    );
    std::fs::write(&file, "just text").unwrap();
    assert!(preview_with("preview-calendar-contacts", "", "calendar", &file).is_none());
}

#[test]
#[cfg(feature = "plugin-calendar-contacts")]
fn contacts_list_names_and_numbers() {
    let sb = Sandbox::new("plugin-vcf");
    let file = sb.touch("a.vcf", "");
    std::fs::write(&file, "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\nORG:Analytical Engines;\nTEL;TYPE=CELL,VOICE:+1 555 0100\nEMAIL;TYPE=INTERNET:ada@example.com\nEND:VCARD\n").unwrap();
    let s = text_of(preview_with(
        "preview-calendar-contacts",
        "",
        "contacts",
        &file,
    ));
    assert!(s.contains("1 contact"), "{s}");
    assert!(
        s.contains("Ada Lovelace") && s.contains("Analytical Engines"),
        "{s}"
    );
    assert!(s.contains("phone (cell): +1 555 0100"), "{s}");
    assert!(s.contains("email: ada@example.com"), "{s}");
}

#[test]
#[cfg(feature = "plugin-documents")]
fn rtf_keeps_the_words() {
    let sb = Sandbox::new("plugin-rtf");
    let file = sb.touch("a.rtf", "");
    std::fs::write(&file, r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}{\colortbl;\red0\green0\blue0;}\f0\fs24 Hello \b world\b0\par Second\tab line \'e9t\'e9 \'80{\*\generator Word;}}").unwrap();
    let t = text_of(preview_with("preview-documents", "", "rtf", &file));
    assert!(t.starts_with("Hello world"), "{t}");
    assert!(t.contains("Second\tline"), "{t}");
    assert!(t.contains("été"), "{t}");
    assert!(t.contains('€'), "{t}");
    assert!(!t.contains("Arial") && !t.contains("Word"), "{t}");
    std::fs::write(&file, "plain words").unwrap();
    assert!(preview_with("preview-documents", "", "rtf", &file).is_none());
}

#[test]
#[cfg(feature = "plugin-documents")]
fn cover_prefers_a_file_called_cover() {
    let sb = Sandbox::new("plugin-cover");
    let file = sb.touch("x", "");
    let pick = |members: &str| {
        let extra = format!("function t() return {{ text = tostring(pick_cover({members})) }} end");
        text_of(preview_with("preview-documents", &extra, "t", &file))
    };
    assert_eq!(
        pick(
            r#"{"OEBPS/ch1.xhtml", "OEBPS/images/b.jpg", "OEBPS/images/Cover.PNG", "OEBPS/images/a.jpg"}"#
        ),
        "OEBPS/images/Cover.PNG"
    );
    assert_eq!(
        pick(r#"{"pages/02.jpg", "pages/01.jpg", "info.txt"}"#),
        "pages/01.jpg"
    );
    assert_eq!(pick(r#"{"a.txt"}"#), "nil");
}

#[test]
#[cfg(feature = "plugin-system")]
fn programs_are_described_when_readelf_exists() {
    if Command::new("readelf").arg("--version").output().is_err() {
        return;
    }
    let s = text_of(preview_with(
        "preview-system",
        "",
        "program",
        Path::new("/bin/sh"),
    ));
    assert!(s.contains("machine:"), "{s}");
    assert!(preview_with("preview-system", "", "program", Path::new("/etc/hostname")).is_none());
}

#[test]
#[cfg(feature = "plugin-torrent")]
fn previews_come_from_enabled_plugins_only() {
    let sb = Sandbox::new("plugin-switch");
    let file = sb.touch("a.torrent", "");
    std::fs::write(&file, b"d4:infod4:name1:x6:lengthi5eee").unwrap();
    let path = file.to_string_lossy().into_owned();
    let ask = || preview(&path, "a.torrent", "torrent", 30, 1);
    assert!(matches!(ask(), Some(Preview::Text { note, .. }) if note == "torrent"));
    crate::state::write_config("settings", r#"{"disabledPlugins":["preview-torrent"]}"#);
    assert!(ask().is_none());
    let listed = list();
    let entry = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "preview-torrent")
        .unwrap();
    assert_eq!(
        (entry["bundled"].clone(), entry["enabled"].clone()),
        (json!(true), json!(false))
    );
}

#[test]
fn a_plugin_in_the_config_folder_replaces_a_bundled_one() {
    let sb = Sandbox::new("plugin-override");
    install(
        &sb,
        "preview-torrent",
        r#"{"previews":[{"id":"t","ext":["torrent"],"function":"p"}]}"#,
        "function p(ctx) return { text = 'mine', note = 'custom' } end",
    );
    let file = sb.touch("a.torrent", "x");
    let path = file.to_string_lossy().into_owned();
    assert!(
        matches!(preview(&path, "a.torrent", "torrent", 1, 1), Some(Preview::Text { text, .. }) if text == "mine")
    );
}

#[test]
fn plugins_switch_by_id_short_name_or_all() {
    let _sb = Sandbox::new("plugin-cli");
    install(
        &_sb,
        "mine",
        r#"{"actions":[{"id":"a"}]}"#,
        "function a() end",
    );
    let off = || disabled_plugins();
    assert!(set_enabled("mine", false).unwrap().contains("mine off"));
    assert_eq!(off(), ["mine"]);
    assert!(
        set_enabled("nonsense", true)
            .unwrap_err()
            .contains("no plugin")
    );
    set_enabled("all", false).unwrap();
    assert!(off().contains(&"mine".to_owned()));
    set_enabled("all", true).unwrap();
    assert!(off().is_empty());
    assert!(!crate::state::read_config("settings").contains("disabledPlugins"));
    assert!(describe().contains("mine"));
    #[cfg(feature = "plugin-torrent")]
    {
        set_enabled("torrent", false).unwrap();
        assert_eq!(off(), ["preview-torrent"]);
    }
}

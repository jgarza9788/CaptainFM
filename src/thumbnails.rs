//! Thumbnails from the freedesktop thumbnail cache (`$XDG_CACHE_HOME/thumbnails/<size>/<md5 of URI>.png`).

use std::path::PathBuf;

use md5::{Digest, Md5};

/// Cache file name for `path`: the MD5 of its percent-encoded `file://` URI.
pub fn cache_name(path: &str) -> String {
    let uri = format!("file://{}", crate::ops::percent_encode_path(path));
    let digest = Md5::digest(uri.as_bytes());
    let mut name = String::with_capacity(36);
    for b in digest {
        name.push_str(&format!("{b:02x}"));
    }
    name.push_str(".png");
    name
}

fn cache_root() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::state::home().join(".cache"))
        .join("thumbnails")
}

/// Largest cached thumbnail for `path`, as a `file://` URL, if one exists.
pub fn lookup(path: &str) -> Option<String> {
    let name = cache_name(path);
    ["x-large", "large", "normal"]
        .iter()
        .map(|size| cache_root().join(size).join(&name))
        .find(|p| p.is_file())
        .map(|p| format!("file://{}", crate::ops::percent_encode_path(&p.to_string_lossy())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Sandbox;

    #[test]
    fn cache_name_matches_the_spec_hash() {
        // md5("file:///home/user/My%20Pics/a.png")
        assert_eq!(cache_name("/home/user/My Pics/a.png"), "0c5d5dbf41ce352ec994d8dee8f783f1.png");
    }

    #[test]
    fn finds_the_largest_cached_size() {
        let sb = Sandbox::new("thumbs");
        let target = "/home/user/pic.png";
        assert_eq!(lookup(target), None);
        let name = cache_name(target);
        sb.touch(&format!(".cache/thumbnails/normal/{name}"), "x");
        assert!(lookup(target).unwrap().contains("/normal/"));
        sb.touch(&format!(".cache/thumbnails/large/{name}"), "x");
        assert!(lookup(target).unwrap().contains("/large/"));
        assert!(lookup(target).unwrap().starts_with("file://"));
    }
}

//! HTML sanitiser for the "rich" BBS rendering mode.
//!
//! See `docs/protocols/bbs.md` §6 for the threat model and the
//! whitelist below.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

fn build_cleaner() -> ammonia::Builder<'static> {
    let mut tags = HashSet::new();
    for t in ["br", "a", "b", "i", "u", "s", "font", "small", "big"] {
        tags.insert(t);
    }

    let mut tag_attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
    tag_attrs.insert("a", ["href"].into_iter().collect());
    tag_attrs.insert("font", ["color", "size"].into_iter().collect());

    let mut schemes = HashSet::new();
    for s in ["http", "https", "ftp", "mailto"] {
        schemes.insert(s);
    }

    let mut b = ammonia::Builder::default();
    b.tags(tags)
        .tag_attributes(tag_attrs)
        .url_schemes(schemes)
        .link_rel(Some("noopener noreferrer"));
    b
}

fn cleaner() -> &'static ammonia::Builder<'static> {
    static C: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    C.get_or_init(build_cleaner)
}

/// Run a single post body through the whitelist sanitiser.
pub fn sanitize_post_html(input: &str) -> String {
    cleaner().clean(input).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_script_tags() {
        let out = sanitize_post_html("hello<script>alert(1)</script>world");
        assert!(!out.contains("<script>"));
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
    }

    #[test]
    fn keeps_allowed_anchor_with_safe_scheme() {
        let out = sanitize_post_html(r#"<a href="https://example.invalid/">x</a>"#);
        assert!(out.contains("https://example.invalid/"));
    }

    #[test]
    fn strips_javascript_uri() {
        let out = sanitize_post_html(r#"<a href="javascript:alert(1)">x</a>"#);
        assert!(!out.contains("javascript:"));
    }

    #[test]
    fn strips_inline_event_handlers() {
        let out = sanitize_post_html(r#"<a href="https://x/" onclick="evil()">x</a>"#);
        assert!(!out.contains("onclick"));
    }

    #[test]
    fn strips_img_tags() {
        let out = sanitize_post_html(r#"<img src="https://x/y.png">"#);
        assert!(!out.contains("<img"));
    }

    #[test]
    fn keeps_br_and_styling() {
        let out = sanitize_post_html("a<br><b>bold</b> <i>it</i>");
        assert!(out.contains("<br>") || out.contains("<br />"));
        assert!(out.contains("<b>bold</b>"));
        assert!(out.contains("<i>it</i>"));
    }

    #[test]
    fn font_color_kept_but_style_dropped() {
        let out = sanitize_post_html(
            r#"<font color="red" style="background:url(javascript:1)">x</font>"#,
        );
        assert!(out.contains("color=\"red\""));
        assert!(!out.contains("style="));
        assert!(!out.contains("javascript:"));
    }
}

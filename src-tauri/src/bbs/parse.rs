//! Subject and dat parsers for the two board families documented in
//! `docs/protocols/bbs.md` §3 (shitaraba) and §4 (2ch-compatible).
//!
//! No HTTP here: callers feed in a decoded UTF-8 body.

use super::{anchor::find_anchors, encoding::unescape_html, types::Post};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectEntry {
    /// Thread "key" (`dat` filename without extension on 2ch,
    /// `cgi` filename without extension on shitaraba — i.e. the
    /// UNIX timestamp at which the thread was created).
    pub key: String,
    pub title: String,
    pub count: u32,
}

/// Parse a shitaraba `subject.txt`.
///
/// Format (per line): `{datNumber}.cgi,{title} ({count})\n`
pub fn parse_shitaraba_subject(body: &str) -> Vec<SubjectEntry> {
    body.lines()
        .filter_map(|line| {
            let (file, rest) = line.split_once(',')?;
            let key = file.strip_suffix(".cgi").unwrap_or(file).to_string();
            let (title, count) = split_title_and_count(rest)?;
            Some(SubjectEntry { key, title, count })
        })
        .collect()
}

/// Parse a 2ch-compatible `subject.txt`.
///
/// Format (per line): `{datNumber}.dat<>{title} ({count})\n`
pub fn parse_ch2_subject(body: &str) -> Vec<SubjectEntry> {
    body.lines()
        .filter_map(|line| {
            let (file, rest) = line.split_once("<>")?;
            let key = file.strip_suffix(".dat").unwrap_or(file).to_string();
            let (title, count) = split_title_and_count(rest)?;
            Some(SubjectEntry { key, title, count })
        })
        .collect()
}

/// Tease apart `"title (123)"` into `("title", 123)`.
/// Accepts both `({n})` and `(n)`. Falls back to 0 if no count present.
fn split_title_and_count(s: &str) -> Option<(String, u32)> {
    let s = s.trim_end();
    if let Some(open) = s.rfind(" (") {
        let (title, count_part) = s.split_at(open);
        let count_str = count_part.trim_start_matches(" (").trim_end_matches(')');
        if let Ok(n) = count_str.parse::<u32>() {
            return Some((title.to_string(), n));
        }
    }
    Some((s.to_string(), 0))
}

/// Parse a shitaraba `rawmode.cgi` body (already UTF-8).
///
/// Format (per line): `{n}<>{name}<>{mail}<>{date+ID}<>{body}<>{title}`
pub fn parse_shitaraba_dat(body: &str) -> Vec<Post> {
    body.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(6, "<>").collect();
            if f.len() < 5 {
                return None;
            }
            let number: u32 = f[0].parse().ok()?;
            let (date, id) = split_date_id(f[3]);
            let mut p = Post {
                number,
                name: clean_text(f[1]),
                mail: f[2].to_string(),
                date,
                id,
                body: clean_body(f[4]),
                thread_title: f.get(5).map(|s| (*s).to_string()).unwrap_or_default(),
            };
            // anchors don't change the post itself, but parsing them here
            // lets us cache a lightweight sanity check.
            let _ = find_anchors(&p.body);
            // shitaraba-only: full-width tilde entity → ～
            p.body = p.body.replace("&#65374;", "～");
            Some(p)
        })
        .collect()
}

/// Parse a 2ch-compatible dat body (already UTF-8).
///
/// Format (per line): `{name}<>{mail}<>{date+ID}<>{body}<>{title}<>...`
/// Numbering is implicit: 1-based line number.
pub fn parse_ch2_dat(body: &str) -> Vec<Post> {
    body.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let f: Vec<&str> = line.splitn(7, "<>").collect();
            if f.len() < 4 {
                return None;
            }
            let (date, id) = split_date_id(f[2]);
            Some(Post {
                number: (i as u32) + 1,
                name: clean_text(f[0]),
                mail: f[1].to_string(),
                date,
                id,
                body: clean_body(f[3]),
                thread_title: f.get(4).map(|s| (*s).to_string()).unwrap_or_default(),
            })
        })
        .collect()
}

fn clean_text(raw: &str) -> String {
    // Names commonly contain </b>...<b> trip wrappers; strip them but keep contents.
    let stripped =
        raw.replace("</b>", "").replace("<b>", "").replace("</B>", "").replace("<B>", "");
    unescape_html(stripped.trim())
}

fn clean_body(raw: &str) -> String {
    // 2ch bodies use ` <br> ` for newlines.
    let with_nl = raw.replace(" <br> ", "\n").replace("<br>", "\n");
    unescape_html(&with_nl)
}

fn split_date_id(field: &str) -> (String, String) {
    if let Some((date, rest)) = field.split_once(" ID:") {
        // ID column may have extra suffixes like " BE:..."; trim at first space.
        let id = rest.split_whitespace().next().unwrap_or("");
        (date.to_string(), id.to_string())
    } else {
        (field.to_string(), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shitaraba_subject_basic() {
        let body = "1672500000.cgi,テストスレ part1 (123)\n1672500100.cgi,雑談 (42)\n";
        let v = parse_shitaraba_subject(body);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].key, "1672500000");
        assert_eq!(v[0].title, "テストスレ part1");
        assert_eq!(v[0].count, 123);
    }

    #[test]
    fn ch2_subject_basic() {
        let body = "1672500000.dat<>テストスレ part1 (123)\n1672500100.dat<>雑談 (42)\n";
        let v = parse_ch2_subject(body);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].key, "1672500000");
        assert_eq!(v[0].title, "テストスレ part1");
        assert_eq!(v[1].count, 42);
    }

    #[test]
    fn shitaraba_dat_basic() {
        let body = "\
1<>名無しさん<>sage<>2026/05/30(土) 12:34:56 ID:abc<>本文1<>テストスレ
2<>名無しさん<><>2026/05/30(土) 12:35:00 ID:def<>レス&lt;test&gt;<>";
        let posts = parse_shitaraba_dat(body);
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].number, 1);
        assert_eq!(posts[0].thread_title, "テストスレ");
        assert_eq!(posts[0].id, "abc");
        assert_eq!(posts[1].body, "レス<test>");
        assert_eq!(posts[1].mail, "");
    }

    #[test]
    fn ch2_dat_basic() {
        let body = "\
名無しさん<>sage<>2026/05/30(土) 12:34:56.78 ID:xxxxxxxx<>こんにちは<>テストスレ<>
名無しさん<><>2026/05/30(土) 12:35:00.10 ID:yyyyyyyy BE:1<>てす<br>と<><><>";
        let posts = parse_ch2_dat(body);
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].number, 1);
        assert_eq!(posts[0].id, "xxxxxxxx");
        assert_eq!(posts[1].number, 2);
        assert_eq!(posts[1].id, "yyyyyyyy");
        assert_eq!(posts[1].body, "てす\nと");
    }

    #[test]
    fn ch2_dat_strips_trip_wrappers() {
        let body = "名無し </b>◆ABC<b> <><>2026 ID:z<>x<>";
        let posts = parse_ch2_dat(body);
        assert_eq!(posts[0].name, "名無し ◆ABC");
    }

    #[test]
    fn shitaraba_tilde_entity_replaced_in_body() {
        let body = "1<>n<>m<>2026 ID:a<>x&#65374;y<>t";
        let posts = parse_shitaraba_dat(body);
        assert_eq!(posts[0].body, "x～y");
    }

    #[test]
    fn subject_with_curly_count_tolerated() {
        // Some servers wrap count in {} or omit it.
        let v = parse_ch2_subject("123.dat<>no count\n");
        assert_eq!(v[0].title, "no count");
        assert_eq!(v[0].count, 0);
    }
}

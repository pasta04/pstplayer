//! Subject and dat parsers for the two board families documented in
//! `docs/protocols/bbs.md` §3 (shitaraba) and §4 (2ch-compatible).
//!
//! No HTTP here: callers feed in a decoded UTF-8 body.

use super::{anchor::find_anchors, encoding::unescape_html, types::BoardSetting, types::Post};
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

/// 同一 `key` のスレッドを除去する (最初の出現を採用)。したらばは
/// 最近書き込まれたスレを subject.txt の **先頭と末尾の両方** に重複して
/// 載せることがある。重複 key が残るとフロントの keyed each が例外を投げて
/// 一覧描画が壊れる (実機 QA で発覚) ため、パーサ側で潰しておく。
fn dedup_by_key(mut entries: Vec<SubjectEntry>) -> Vec<SubjectEntry> {
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert(e.key.clone()));
    entries
}

/// Parse a shitaraba `subject.txt`.
///
/// Format (per line): `{datNumber}.cgi,{title} ({count})\n`
pub fn parse_shitaraba_subject(body: &str) -> Vec<SubjectEntry> {
    let entries = body
        .lines()
        .filter_map(|line| {
            let (file, rest) = line.split_once(',')?;
            let key = file.strip_suffix(".cgi").unwrap_or(file).to_string();
            let (title, count) = split_title_and_count(rest)?;
            Some(SubjectEntry { key, title, count })
        })
        .collect();
    dedup_by_key(entries)
}

/// Parse a 2ch-compatible `subject.txt`.
///
/// Format (per line): `{datNumber}.dat<>{title} ({count})\n`
pub fn parse_ch2_subject(body: &str) -> Vec<SubjectEntry> {
    let entries = body
        .lines()
        .filter_map(|line| {
            let (file, rest) = line.split_once("<>")?;
            let key = file.strip_suffix(".dat").unwrap_or(file).to_string();
            let (title, count) = split_title_and_count(rest)?;
            Some(SubjectEntry { key, title, count })
        })
        .collect();
    dedup_by_key(entries)
}

/// Tease apart `"title(123)"` / `"title (123)"` into `("title", 123)`.
///
/// 2ch は `タイトル (123)` (括弧前にスペース)、したらばは `タイトル(123)`
/// (スペース無し) と流儀が違う。スペースの有無に依存せず、**末尾の
/// `(数字)` グループ** を末尾から探して数える。括弧をタイトルに含むスレ
/// (例 `グラブル(神)(1000)`) でも最後の `(数字)` だけを count にする。
/// 末尾が `(数字)` でない場合は count=0 でタイトルそのまま。
fn split_title_and_count(s: &str) -> Option<(String, u32)> {
    let s = s.trim_end();
    if let Some(open) = s.rfind('(') {
        if s.ends_with(')') {
            // `(` と `)` は ASCII 1 byte なので byte index で安全に切れる。
            let count_str = &s[open + 1..s.len() - 1];
            if !count_str.is_empty() && count_str.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(n) = count_str.parse::<u32>() {
                    return Some((s[..open].trim_end().to_string(), n));
                }
            }
        }
    }
    Some((s.to_string(), 0))
}

/// `KEY=VALUE` 形式の板設定 (SETTING.TXT / setting.cgi) を解釈する。
/// `max_res` は したらば `BBS_THREAD_STOP` と 2ch 互換 `BBS_RES_MAX` の
/// どちらか存在する方を採用 (両方あれば THREAD_STOP 優先)。`body` は
/// 呼び出し側で適切な文字コード (したらば=EUC-JP, 2ch=Shift_JIS) から
/// デコード済みの文字列を渡すこと。
pub fn parse_board_setting(body: &str) -> BoardSetting {
    let mut s = BoardSetting::default();
    let mut res_max: Option<u32> = None;
    let mut thread_stop: Option<u32> = None;
    for line in body.lines() {
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        match key {
            "BBS_THREAD_STOP" => thread_stop = val.parse().ok(),
            "BBS_RES_MAX" => res_max = val.parse().ok(),
            "BBS_NONAME_NAME" => s.default_name = val.to_string(),
            "BBS_TITLE" => s.title = val.to_string(),
            _ => {}
        }
    }
    s.max_res = thread_stop.or(res_max).unwrap_or(0);
    s
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
    let stripped = raw
        .replace("</b>", "")
        .replace("<b>", "")
        .replace("</B>", "")
        .replace("<B>", "");
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
    fn shitaraba_subject_no_space_real_format() {
        // 実機の subject.txt はスペース無し `タイトル(レス数)`。
        // 旧実装は count=0・タイトルに「(484)」混入になっていた。
        let body = "1760675037.cgi,ナイトレン(484)\n1696385564.cgi,90(1000)\n";
        let v = parse_shitaraba_subject(body);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title, "ナイトレン");
        assert_eq!(v[0].count, 484);
        // 数字だけのタイトルでも count と取り違えない。
        assert_eq!(v[1].title, "90");
        assert_eq!(v[1].count, 1000);
    }

    #[test]
    fn subject_title_with_parens_keeps_last_count() {
        let v = parse_shitaraba_subject("1.cgi,グラブル(神)(1000)\n");
        assert_eq!(v[0].title, "グラブル(神)");
        assert_eq!(v[0].count, 1000);
    }

    #[test]
    fn shitaraba_subject_dedups_duplicate_key() {
        // したらばは最新スレを先頭と末尾の両方に載せることがある。
        // 重複 key を残すとフロントの keyed each が壊れるので除去する。
        let body = "1760675037.cgi,ナイトレン(484)\n1696385564.cgi,90(1000)\n1760675037.cgi,ナイトレン(484)\n";
        let v = parse_shitaraba_subject(body);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].key, "1760675037");
        assert_eq!(v[1].key, "1696385564");
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
    fn board_setting_shitaraba_thread_stop() {
        let body = "TOP=https://jbbs.shitaraba.net/internet/22667/\nBBS_THREAD_STOP=1000\nBBS_NONAME_NAME=名無しさん\nBBS_TITLE=どれいくch\n";
        let s = parse_board_setting(body);
        assert_eq!(s.max_res, 1000);
        assert_eq!(s.default_name, "名無しさん");
        assert_eq!(s.title, "どれいくch");
    }

    #[test]
    fn board_setting_ch2_res_max() {
        let body = "BBS_TITLE=避難所\nBBS_NONAME_NAME=名無しの麺類\nBBS_RES_MAX=1000\n";
        let s = parse_board_setting(body);
        assert_eq!(s.max_res, 1000);
        assert_eq!(s.default_name, "名無しの麺類");
    }

    #[test]
    fn board_setting_missing_max_is_zero() {
        let s = parse_board_setting("BBS_TITLE=x\n");
        assert_eq!(s.max_res, 0);
    }

    #[test]
    fn subject_with_curly_count_tolerated() {
        // Some servers wrap count in {} or omit it.
        let v = parse_ch2_subject("123.dat<>no count\n");
        assert_eq!(v[0].title, "no count");
        assert_eq!(v[0].count, 0);
    }
}

//! YP / PeerCast チャンネルに対するお気に入りルール。
//!
//! ルールは複数持てて、上から評価される。最初にマッチしたものを採用。
//! 設定項目:
//! - `name`: 識別名 (UI 表示用、マッチには使わない)
//! - `pattern` + `match_name`/`match_genre`/`match_desc`/`match_comment`:
//!   1 つの部分一致パターンを、チェックしたフィールドのどれか (OR) に
//!   当てる (PeCaRecorder の基本検索と同じ考え方)。`|` 区切りで OR。
//! - `channel_name` / `genre` / `desc` / `comment`: 旧形式 (フィールド別
//!   パターンの AND)。`pattern` が空のときだけ使われる。config 読み込み
//!   時に単一フィールドのルールは新形式へ自動移行される (migrate_rules)。
//! - `pin_top`: チャンネル一覧で上位に固定する
//! - `auto_record`: 視聴開始時に自動で録画を始める
//! - `color`: 一覧での背景色 (CSS color 文字列 / 空欄なら標準色)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FavoritesConfig {
    /// ルールは設定ファイル / UI 上の並びそのまま。順序で評価される
    /// ため、より具体的なルールを上に書く運用にする。
    #[serde(default)]
    pub rules: Vec<FavoriteRule>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FavoriteRule {
    #[serde(default)]
    pub name: String,
    /// マッチパターン (部分一致 / 大文字小文字無視 / `|` 区切り OR)。
    /// `match_*` でチェックしたフィールドのどれかに一致すればマッチ。
    /// 空欄なら旧形式 (下の channel_name 等) にフォールバックする。
    #[serde(default)]
    pub pattern: String,
    /// pattern をチャンネル名に当てるか。既定 ON。
    #[serde(default = "default_true")]
    pub match_name: bool,
    #[serde(default)]
    pub match_genre: bool,
    #[serde(default)]
    pub match_desc: bool,
    #[serde(default)]
    pub match_comment: bool,
    /// 旧形式のフィールド別パターン。`pattern` が空のときだけ使われる。
    #[serde(default)]
    pub channel_name: String,
    #[serde(default)]
    pub genre: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub pin_top: bool,
    #[serde(default)]
    pub auto_record: bool,
    /// 旧フィールド。背景色。新しい設定 UI では `background` で扱うが、
    /// 古い config.toml との互換のため deserialize は `color` で受ける。
    /// 新規書き出しでは `background` のみで足りるが、両方持ち続ける。
    /// `background` が空かつ `color` に値があれば `background` として
    /// 解釈する (`effective_background` ヘルパ参照)。
    #[serde(default)]
    pub color: String,
    /// 行の背景色 (CSS color)。空なら未指定 (テーマ既定)。
    #[serde(default)]
    pub background: String,
    /// 行の文字色 (CSS color)。空なら未指定。
    #[serde(default)]
    pub text_color: String,
    /// このルールにマッチした時の挙動。
    #[serde(default)]
    pub action: FavoriteAction,
}

impl FavoriteRule {
    /// 互換用: `background` が空なら旧 `color` を使う。
    pub fn effective_background(&self) -> &str {
        if !self.background.is_empty() {
            &self.background
        } else {
            &self.color
        }
    }
}

/// マッチしたルールがチャンネルにどう影響するか。
///
/// - `Show` (既定): ハブで通常表示。`pin_top` / `auto_record` / 色は
///   他フィールドどおりに適用
/// - `Ignore`: ハブの「すべて」「お気に入り」タブから非表示。専用の
///   「非表示」タブで確認はできる。録画 / 視聴 spawn の対象外
/// - `Block`: 完全ブロック。表示しない、視聴 spawn しない、録画しない。
///   別ルールで `auto_record = true` でも、優先順位がこちらに当たれば
///   録画されない
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FavoriteAction {
    #[default]
    Show,
    Ignore,
    Block,
}

/// 「お気に入り判定」できる対象。`YpEntry` と `ChannelInfo` の両方を
/// 1 つの関数 (`matches`) で扱うための統一インタフェース。
pub trait MatchTarget {
    fn ch_name(&self) -> &str;
    fn ch_genre(&self) -> &str;
    fn ch_desc(&self) -> &str;
    fn ch_comment(&self) -> &str;
}

impl MatchTarget for crate::peercast::yp::YpEntry {
    fn ch_name(&self) -> &str {
        &self.name
    }
    fn ch_genre(&self) -> &str {
        &self.genre
    }
    fn ch_desc(&self) -> &str {
        &self.desc
    }
    fn ch_comment(&self) -> &str {
        &self.comment
    }
}

impl MatchTarget for crate::peercast::types::ChannelInfo {
    fn ch_name(&self) -> &str {
        &self.name
    }
    fn ch_genre(&self) -> &str {
        &self.genre
    }
    fn ch_desc(&self) -> &str {
        &self.desc
    }
    fn ch_comment(&self) -> &str {
        &self.comment
    }
}

/// 部分一致 / 大文字小文字無視 / Unicode は素のまま。
/// パイプ区切りの OR をサポート: `"foo|bar|baz"` は「foo か bar か baz の
/// どれかを含む」。PeCaRecorder の検索パターンの最頻形式に合わせる。
fn part(needle: &str, hay: &str) -> bool {
    let n = needle.trim();
    if n.is_empty() {
        return true;
    }
    let hay_lc = hay.to_lowercase();
    n.split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .any(|alt| hay_lc.contains(&alt.to_lowercase()))
}

/// 1 ルールに対する判定。
///
/// - 新形式 (`pattern` 非空): チェックした対象フィールドのどれか (OR) に
///   pattern が部分一致すればマッチ。対象が 1 つも無ければマッチしない。
/// - 旧形式 (`pattern` 空): フィールド別パターンの AND。空欄は
///   ワイルドカード (= 全部空なら何にでもマッチ)。
pub fn matches<T: MatchTarget + ?Sized>(rule: &FavoriteRule, t: &T) -> bool {
    let p = rule.pattern.trim();
    if !p.is_empty() {
        let mut hays: Vec<&str> = Vec::new();
        if rule.match_name {
            hays.push(t.ch_name());
        }
        if rule.match_genre {
            hays.push(t.ch_genre());
        }
        if rule.match_desc {
            hays.push(t.ch_desc());
        }
        if rule.match_comment {
            hays.push(t.ch_comment());
        }
        return hays.into_iter().any(|hay| part(p, hay));
    }
    part(&rule.channel_name, t.ch_name())
        && part(&rule.genre, t.ch_genre())
        && part(&rule.desc, t.ch_desc())
        && part(&rule.comment, t.ch_comment())
}

/// 旧形式ルールの自動移行。`pattern` が空で、旧フィールドのうち
/// **ちょうど 1 つ**だけ使われている場合、それを `pattern` + 対象
/// フィールドへ移す (意味が完全に保存できるのは単一フィールドのとき
/// だけ。複数フィールドの AND は旧形式のまま残す)。config 読み込み時に
/// 呼ばれる。
pub fn migrate_rules(rules: &mut [FavoriteRule]) {
    for r in rules {
        if !r.pattern.trim().is_empty() {
            continue;
        }
        let used = [&r.channel_name, &r.genre, &r.desc, &r.comment];
        let non_empty = used.iter().filter(|v| !v.trim().is_empty()).count();
        if non_empty != 1 {
            continue;
        }
        let (pattern, flags) = if !r.channel_name.trim().is_empty() {
            (r.channel_name.clone(), (true, false, false, false))
        } else if !r.genre.trim().is_empty() {
            (r.genre.clone(), (false, true, false, false))
        } else if !r.desc.trim().is_empty() {
            (r.desc.clone(), (false, false, true, false))
        } else {
            (r.comment.clone(), (false, false, false, true))
        };
        r.pattern = pattern;
        (r.match_name, r.match_genre, r.match_desc, r.match_comment) = flags;
        r.channel_name.clear();
        r.genre.clear();
        r.desc.clear();
        r.comment.clear();
    }
}

/// 先頭から評価して最初にマッチしたルールを返す。
pub fn first_match<'a, T: MatchTarget + ?Sized>(
    rules: &'a [FavoriteRule],
    t: &T,
) -> Option<&'a FavoriteRule> {
    rules.iter().find(|r| matches(r, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peercast::yp::YpEntry;

    fn yp(name: &str, genre: &str) -> YpEntry {
        YpEntry {
            name: name.into(),
            genre: genre.into(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_rule_matches_everything() {
        let r = FavoriteRule::default();
        assert!(matches(&r, &yp("foo", "music")));
        assert!(matches(&r, &yp("", "")));
    }

    #[test]
    fn name_substring() {
        let r = FavoriteRule {
            channel_name: "mox".into(),
            ..Default::default()
        };
        assert!(matches(&r, &yp("MOXch", "")));
        assert!(matches(&r, &yp("mox-ch", "")));
        assert!(!matches(&r, &yp("other", "")));
    }

    #[test]
    fn pattern_or_across_checked_fields() {
        let r = FavoriteRule {
            pattern: "rta".into(),
            match_name: true,
            match_genre: true,
            ..Default::default()
        };
        assert!(matches(&r, &yp("MarioRTA", "")));
        assert!(matches(&r, &yp("", "RTA")));
        assert!(!matches(&r, &yp("other", "music")));
    }

    #[test]
    fn pattern_without_targets_never_matches() {
        let r = FavoriteRule {
            pattern: "foo".into(),
            match_name: false,
            ..Default::default()
        };
        assert!(!matches(&r, &yp("foo", "foo")));
    }

    #[test]
    fn legacy_single_field_rule_migrates_to_pattern() {
        let mut rules = vec![
            FavoriteRule {
                channel_name: "foo|bar".into(),
                ..Default::default()
            },
            FavoriteRule {
                genre: "game".into(),
                ..Default::default()
            },
            // 複数フィールド AND は旧形式のまま残す。
            FavoriteRule {
                channel_name: "foo".into(),
                genre: "game".into(),
                ..Default::default()
            },
        ];
        migrate_rules(&mut rules);
        assert_eq!(rules[0].pattern, "foo|bar");
        assert!(rules[0].match_name);
        assert!(!rules[0].match_genre);
        assert!(rules[0].channel_name.is_empty());
        assert_eq!(rules[1].pattern, "game");
        assert!(!rules[1].match_name);
        assert!(rules[1].match_genre);
        assert!(rules[2].pattern.is_empty());
        assert_eq!(rules[2].channel_name, "foo");
    }

    #[test]
    fn legacy_rule_deserializes_with_match_name_default_true() {
        let r: FavoriteRule = toml::from_str("channel_name = \"foo\"").unwrap();
        assert!(r.match_name);
        assert!(matches(&r, &yp("foo ch", "")));
    }

    #[test]
    fn name_or_pattern() {
        let r = FavoriteRule {
            channel_name: "foo|bar|baz".into(),
            ..Default::default()
        };
        assert!(matches(&r, &yp("foobar", "")));
        assert!(matches(&r, &yp("just bar inside", "")));
        assert!(matches(&r, &yp("BAZTASTIC", "")));
        assert!(!matches(&r, &yp("qux", "")));
    }

    #[test]
    fn or_pattern_trims_whitespace() {
        let r = FavoriteRule {
            channel_name: " foo | bar ".into(),
            ..Default::default()
        };
        assert!(matches(&r, &yp("FoO", "")));
        assert!(matches(&r, &yp("bar", "")));
        assert!(!matches(&r, &yp("baz", "")));
    }

    #[test]
    fn name_and_genre_are_anded() {
        let r = FavoriteRule {
            channel_name: "mox".into(),
            genre: "music".into(),
            ..Default::default()
        };
        assert!(matches(&r, &yp("MOXch", "music live")));
        assert!(!matches(&r, &yp("MOXch", "game")));
        assert!(!matches(&r, &yp("other", "music live")));
    }

    #[test]
    fn effective_background_falls_back_to_color() {
        let r = FavoriteRule {
            color: "#abcdef".into(),
            background: String::new(),
            ..Default::default()
        };
        assert_eq!(r.effective_background(), "#abcdef");

        let r = FavoriteRule {
            color: "#abcdef".into(),
            background: "#123456".into(),
            ..Default::default()
        };
        assert_eq!(r.effective_background(), "#123456");
    }

    #[test]
    fn action_defaults_to_show() {
        let r = FavoriteRule::default();
        assert_eq!(r.action, FavoriteAction::Show);
    }

    #[test]
    fn action_deserialises_lowercase() {
        let s = r##"
            name = "test"
            channel_name = "foo"
            action = "ignore"
        "##;
        let r: FavoriteRule = toml::from_str(s).unwrap();
        assert_eq!(r.action, FavoriteAction::Ignore);

        let s = r##"
            name = "test"
            channel_name = "foo"
            action = "block"
        "##;
        let r: FavoriteRule = toml::from_str(s).unwrap();
        assert_eq!(r.action, FavoriteAction::Block);
    }

    #[test]
    fn legacy_color_field_is_still_parsed() {
        let s = r##"
            name = "old"
            channel_name = "x"
            color = "#aabbcc"
        "##;
        let r: FavoriteRule = toml::from_str(s).unwrap();
        assert_eq!(r.color, "#aabbcc");
        assert_eq!(r.effective_background(), "#aabbcc");
    }

    #[test]
    fn first_match_returns_first_rule() {
        let rules = vec![
            FavoriteRule {
                name: "B".into(),
                channel_name: "mox".into(),
                ..Default::default()
            },
            FavoriteRule {
                name: "A".into(),
                ..Default::default()
            },
        ];
        // どちらにもマッチするケース → 上にある B が選ばれる
        assert_eq!(first_match(&rules, &yp("MOXch", "")).unwrap().name, "B");
        // 1 番目に当たらないものは 2 番目の汎用ルールに当たる
        assert_eq!(first_match(&rules, &yp("other", "")).unwrap().name, "A");
    }
}

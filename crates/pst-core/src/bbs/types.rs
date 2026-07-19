use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoardKind {
    Shitaraba,
    Ch2Compat,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub key: String,
    pub title: String,
    pub count: u32,
}

// フロント (api.ts) は camelCase の `threadTitle` を読む。rename_all が無いと
// snake_case (`thread_title`) のまま IPC/REST に流れ、フロントでは全レスの
// threadTitle が undefined になる (実機で `p.threadTitle.trim()` が throw し、
// スレタイ横のレス数がまとめて固着した実績あり。BbsConfig と同様に camelCase
// へ統一する)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub number: u32,
    pub name: String,
    pub mail: String,
    pub date: String,
    pub id: String,
    pub body: String,
    pub thread_title: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchState {
    pub last_modified: Option<String>,
    pub last_byte: u64,
    pub last_count: u32,
    /// この応答が「スレ全体のスナップショット」なら true。フロントは
    /// true なら手元のレス一覧を**置換**、false なら**追記**する。
    /// 増分取得が使えず全件を返した場合 (サーバが Range 無視 / 416 /
    /// dat 再構築検知) に、フロントが誤って全レスを二重追記して
    /// レス番号キーの {#each} が落ちる事故を防ぐ (実機 QA:
    /// komokomo.ddns.net)。
    #[serde(default)]
    pub full_reload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostRequest {
    pub name: String,
    pub mail: String,
    pub body: String,
}

/// 板の設定 (SETTING.TXT / setting.cgi)。スレッドの最大レス数を中心に
/// 必要な項目だけ抜き出す。`max_res` が 0 のときは「取得できなかった /
/// 設定なし」を表し、呼び出し側はデフォルト (通常 1000) にフォールバック
/// する。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardSetting {
    /// 1 スレッドの最大レス数。したらば `BBS_THREAD_STOP` /
    /// 2ch 互換 `BBS_RES_MAX`。
    pub max_res: u32,
    /// デフォルト名無し (`BBS_NONAME_NAME`)。空の場合あり。
    pub default_name: String,
    /// 板タイトル (`BBS_TITLE`)。
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// フロント (api.ts の Post / FetchState / BoardSetting interface) は
    /// camelCase フィールドを読む。serde の出力が snake_case に戻ると
    /// threadTitle undefined → スレタイ横レス数の固着が再発するため、
    /// JSON のフィールド名そのものを回帰テストで固定する。
    #[test]
    fn bbs_types_serialize_camel_case() {
        let post = serde_json::to_value(Post::default()).unwrap();
        assert!(
            post.get("threadTitle").is_some(),
            "Post must serialize threadTitle: {post}"
        );
        assert!(post.get("thread_title").is_none());

        let state = serde_json::to_value(FetchState::default()).unwrap();
        for key in ["lastModified", "lastByte", "lastCount"] {
            assert!(
                state.get(key).is_some(),
                "FetchState must serialize {key}: {state}"
            );
        }

        let setting = serde_json::to_value(BoardSetting::default()).unwrap();
        for key in ["maxRes", "defaultName"] {
            assert!(
                setting.get(key).is_some(),
                "BoardSetting must serialize {key}: {setting}"
            );
        }
    }
}

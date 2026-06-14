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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
pub struct FetchState {
    pub last_modified: Option<String>,
    pub last_byte: u64,
    pub last_count: u32,
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
pub struct BoardSetting {
    /// 1 スレッドの最大レス数。したらば `BBS_THREAD_STOP` /
    /// 2ch 互換 `BBS_RES_MAX`。
    pub max_res: u32,
    /// デフォルト名無し (`BBS_NONAME_NAME`)。空の場合あり。
    pub default_name: String,
    /// 板タイトル (`BBS_TITLE`)。
    pub title: String,
}

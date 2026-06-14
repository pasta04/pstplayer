use pst_core::bbs::{
    ch2::Ch2Client,
    parse::SubjectEntry,
    router::{self},
    shitaraba::ShitarabaClient,
    types::{BoardKind, BoardSetting, FetchState, Post, PostRequest},
};
use pst_core::util::errors::{AppError, IpcError};

fn classify(url: &str) -> Result<BoardKind, IpcError> {
    router::classify(url)
        .ok_or_else(|| AppError::InvalidUrl(format!("unknown board URL: {url}")))
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_threads(board_url: String) -> Result<Vec<SubjectEntry>, IpcError> {
    match classify(&board_url)? {
        BoardKind::Shitaraba => {
            ShitarabaClient::new().list_threads(&board_url).await.map_err(Into::into)
        }
        BoardKind::Ch2Compat => Ch2Client::new().list_threads(&board_url).await.map_err(Into::into),
    }
}

#[tauri::command]
pub async fn fetch_thread(
    thread_url: String,
    prev: Option<FetchState>,
) -> Result<(Vec<Post>, FetchState), IpcError> {
    match classify(&thread_url)? {
        BoardKind::Shitaraba => ShitarabaClient::new()
            .fetch_thread(&thread_url, prev.as_ref())
            .await
            .map_err(Into::into),
        BoardKind::Ch2Compat => {
            Ch2Client::new().fetch_thread(&thread_url, prev.as_ref()).await.map_err(Into::into)
        }
    }
}

#[tauri::command]
pub async fn post_to_thread(thread_url: String, req: PostRequest) -> Result<(), IpcError> {
    match classify(&thread_url)? {
        BoardKind::Shitaraba => {
            ShitarabaClient::new().post(&thread_url, &req).await.map_err(Into::into)
        }
        BoardKind::Ch2Compat => Ch2Client::new().post(&thread_url, &req).await.map_err(Into::into),
    }
}

#[tauri::command]
pub fn classify_board(url: String) -> Result<BoardKind, IpcError> {
    classify(&url)
}

/// 板の SETTING (最大レス数 = したらば BBS_THREAD_STOP / 2ch BBS_RES_MAX)
/// を取得する。満レス判定 (自動スレ移動) に使う。スレ URL でも板 URL でも可。
#[tauri::command]
pub async fn fetch_board_setting(url: String) -> Result<BoardSetting, IpcError> {
    match classify(&url)? {
        BoardKind::Shitaraba => {
            ShitarabaClient::new().fetch_setting(&url).await.map_err(Into::into)
        }
        BoardKind::Ch2Compat => Ch2Client::new().fetch_setting(&url).await.map_err(Into::into),
    }
}

/// 任意の (スレ or 板) URL から、その板のトップ URL を正規化して返す。
/// スレ一覧ウィンドウや自動スレ移動の起点に使う。
/// - したらば: `https://jbbs.shitaraba.net/{cat}/{board}/`
/// - 2ch 互換: `https://{host}/{board}/`
#[tauri::command]
pub fn board_url_of(url: String) -> Result<String, IpcError> {
    use pst_core::bbs::url::{parse_ch2, parse_shitaraba};
    match classify(&url)? {
        BoardKind::Shitaraba => {
            let u = parse_shitaraba(&url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a shitaraba URL: {url}")))?;
            Ok(format!("https://jbbs.shitaraba.net/{}/{}/", u.category, u.board_id))
        }
        BoardKind::Ch2Compat => {
            let u = parse_ch2(&url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {url}")))?;
            Ok(format!("https://{}/{}/", u.host, u.board))
        }
    }
}

/// 板 URL (or スレ URL) と スレッド key から、その板の流儀に合った
/// canonical なスレッド URL を組み立てる。スレ一覧での選択や自動スレ
/// 移動で「板 + key → スレ URL」を作るのに使う。`${base}/${key}/` を
/// フロントで素朴に繋ぐと 2ch 互換 (`/test/read.cgi/` が必要) で壊れる
/// ため、板種別ごとにバックエンドで構築する。
#[tauri::command]
pub fn thread_url_of(board_url: String, key: String) -> Result<String, IpcError> {
    use pst_core::bbs::url::{parse_ch2, parse_shitaraba};
    // key は数字のみ許可 (URL/パス注入対策)。
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::InvalidUrl(format!("invalid thread key: {key}")).into());
    }
    match classify(&board_url)? {
        BoardKind::Shitaraba => {
            let u = parse_shitaraba(&board_url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a shitaraba URL: {board_url}")))?;
            Ok(format!(
                "https://jbbs.shitaraba.net/bbs/read.cgi/{}/{}/{}/",
                u.category, u.board_id, key
            ))
        }
        BoardKind::Ch2Compat => {
            let u = parse_ch2(&board_url)
                .ok_or_else(|| AppError::InvalidUrl(format!("not a 2ch URL: {board_url}")))?;
            Ok(format!("https://{}/test/read.cgi/{}/{}/", u.host, u.board, key))
        }
    }
}

#[tauri::command]
pub fn sanitize_html(html: String) -> String {
    pst_core::bbs::sanitize::sanitize_post_html(&html)
}

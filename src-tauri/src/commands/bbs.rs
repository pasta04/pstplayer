use pst_core::bbs::{
    ch2::Ch2Client,
    parse::SubjectEntry,
    router::{self},
    shitaraba::ShitarabaClient,
    types::{BoardKind, FetchState, Post, PostRequest},
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

#[tauri::command]
pub fn sanitize_html(html: String) -> String {
    pst_core::bbs::sanitize::sanitize_post_html(&html)
}

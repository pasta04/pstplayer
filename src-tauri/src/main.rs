// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 単一バイナリ・2 モード起動。`--server` (または `serve`) 付きで
    // 起動された場合は GUI を立ち上げず、バックグラウンドのリレーサーバ
    // (pst-server) として常駐する (YP 一覧 API / Web UI / 録画 / HLS /
    // BBS proxy)。引数なし・URL 引数などの通常起動は従来どおりデスクトップ
    // GUI (ハブ / 視聴)。
    //
    // GUI を一切触る前にモードを判定するので、Tauri ランタイムと
    // 二重にランタイムを張ることはない。
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--server" || a == "serve") {
        let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
        let code = rt.block_on(pst_server::serve::run(args));
        std::process::exit(i32::from(code));
    }
    pstplayer_lib::run();
}

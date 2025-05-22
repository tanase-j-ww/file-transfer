use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use local_ip_address::local_ip;
use rfd::FileDialog;
use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

// ファイル転送用のポート
const FILE_TRANSFER_PORT: u16 = 8080;

// コマンドライン引数の定義
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// サーバーモード（ファイル受信）
    Server {
        /// ホットキー（例: "ctrl+shift+r"）
        #[arg(short = 'k', long, default_value = "ctrl+shift+r")]
        hotkey: String,
    },
    /// クライアントモード（ファイル送信）
    Client {
        /// サーバーのIPアドレス
        #[arg(short, long)]
        server: Option<String>,

        /// ホットキー（例: "ctrl+shift+s"）
        #[arg(short = 'k', long, default_value = "ctrl+shift+s")]
        hotkey: String,
    },
}

// ホットキー文字列をパースする関数
fn parse_hotkey(hotkey_str: &str) -> Result<HotKey> {
    let parts: Vec<&str> = hotkey_str.split('+').collect();
    let mut modifiers = Modifiers::empty();
    let mut code = None;

    for part in parts {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" => modifiers |= Modifiers::ALT,
            "meta" | "cmd" | "command" | "win" | "windows" => modifiers |= Modifiers::META,
            key => {
                // キーコードの解析
                code = Some(match key {
                    "a" => Code::KeyA,
                    "b" => Code::KeyB,
                    "c" => Code::KeyC,
                    "d" => Code::KeyD,
                    "e" => Code::KeyE,
                    "f" => Code::KeyF,
                    "g" => Code::KeyG,
                    "h" => Code::KeyH,
                    "i" => Code::KeyI,
                    "j" => Code::KeyJ,
                    "k" => Code::KeyK,
                    "l" => Code::KeyL,
                    "m" => Code::KeyM,
                    "n" => Code::KeyN,
                    "o" => Code::KeyO,
                    "p" => Code::KeyP,
                    "q" => Code::KeyQ,
                    "r" => Code::KeyR,
                    "s" => Code::KeyS,
                    "t" => Code::KeyT,
                    "u" => Code::KeyU,
                    "v" => Code::KeyV,
                    "w" => Code::KeyW,
                    "x" => Code::KeyX,
                    "y" => Code::KeyY,
                    "z" => Code::KeyZ,
                    "0" => Code::Digit0,
                    "1" => Code::Digit1,
                    "2" => Code::Digit2,
                    "3" => Code::Digit3,
                    "4" => Code::Digit4,
                    "5" => Code::Digit5,
                    "6" => Code::Digit6,
                    "7" => Code::Digit7,
                    "8" => Code::Digit8,
                    "9" => Code::Digit9,
                    "f1" => Code::F1,
                    "f2" => Code::F2,
                    "f3" => Code::F3,
                    "f4" => Code::F4,
                    "f5" => Code::F5,
                    "f6" => Code::F6,
                    "f7" => Code::F7,
                    "f8" => Code::F8,
                    "f9" => Code::F9,
                    "f10" => Code::F10,
                    "f11" => Code::F11,
                    "f12" => Code::F12,
                    _ => anyhow::bail!("不明なキーコード: {}", key),
                });
            }
        }
    }

    if let Some(code) = code {
        Ok(HotKey::new(Some(modifiers), code))
    } else {
        anyhow::bail!("キーコードが指定されていません")
    }
}

// サーバーモード（ファイル受信）の実装
async fn run_server(hotkey_str: &str) -> Result<()> {
    println!("サーバーモード（ファイル受信）を開始します");
    println!("ホットキー: {}", hotkey_str);

    // ローカルIPアドレスの取得
    let ip = local_ip()?;
    println!("ローカルIPアドレス: {}", ip);

    // TCPリスナーの作成
    let addr = SocketAddr::from(([0, 0, 0, 0], FILE_TRANSFER_PORT));
    let listener = TcpListener::bind(addr).await?;
    println!("ポート {} でリッスン中", FILE_TRANSFER_PORT);

    // ホットキーマネージャーの初期化
    let hotkey_manager = GlobalHotKeyManager::new().unwrap();
    let hotkey = parse_hotkey(hotkey_str)?;
    hotkey_manager.register(hotkey).unwrap();

    // ファイル保存先の共有状態
    let save_path = Arc::new(Mutex::new(None::<PathBuf>));
    let save_path_clone = save_path.clone();

    // ホットキーイベントの監視
    let hotkey_channel = GlobalHotKeyEvent::receiver();

    // 接続処理用のチャネル
    let (tx, mut rx) = mpsc::channel::<TcpStream>(10);
    let tx_clone = tx.clone();

    // 接続受付ループ
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    println!("新しい接続: {}", addr);
                    if let Err(e) = tx_clone.send(socket).await {
                        eprintln!("ソケットの送信に失敗: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("接続の受付に失敗: {}", e);
                }
            }
        }
    });

    println!("ファイル転送サーバーを起動しました");
    println!("ホットキー {} を押すと保存先を選択できます", hotkey_str);

    // メインループ
    loop {
        // ホットキーイベントの確認
        if let Ok(event) = hotkey_channel.try_recv() {
            if event.id == hotkey.id() {
                println!("ホットキーが押されました");

                // 保存先の選択
                if let Some(path) = FileDialog::new()
                    .set_title("ファイルの保存先フォルダを選択")
                    .pick_folder()
                {
                    println!("保存先を選択: {:?}", path);
                    *save_path.lock().unwrap() = Some(path);
                }
            }
        }

        // 新しい接続の確認
        if let Ok(mut socket) = rx.try_recv() {
            println!("ファイル転送の開始");

            // 保存先の確認
            let save_dir = save_path_clone.lock().unwrap().clone();

            if let Some(save_dir) = save_dir {
                // ファイル名とデータの受信
                let mut filename_len_buf = [0u8; 4];
                if let Err(e) = socket.read_exact(&mut filename_len_buf).await {
                    eprintln!("ファイル名の長さの読み取りに失敗: {}", e);
                    continue;
                }
                let filename_len = u32::from_be_bytes(filename_len_buf) as usize;

                let mut filedata_len_buf = [0u8; 4];
                if let Err(e) = socket.read_exact(&mut filedata_len_buf).await {
                    eprintln!("ファイルデータの長さの読み取りに失敗: {}", e);
                    continue;
                }
                let filedata_len = u32::from_be_bytes(filedata_len_buf) as usize;

                let mut filename_buf = vec![0u8; filename_len];
                if let Err(e) = socket.read_exact(&mut filename_buf).await {
                    eprintln!("ファイル名の読み取りに失敗: {}", e);
                    continue;
                }
                let filename = match String::from_utf8(filename_buf) {
                    Ok(name) => name,
                    Err(e) => {
                        eprintln!("ファイル名のUTF-8変換に失敗: {}", e);
                        continue;
                    }
                };

                let mut filedata = vec![0u8; filedata_len];
                if let Err(e) = socket.read_exact(&mut filedata).await {
                    eprintln!("ファイルデータの読み取りに失敗: {}", e);
                    continue;
                }

                // ファイルの保存
                let save_path = save_dir.join(&filename);
                if let Err(e) = fs::write(&save_path, &filedata) {
                    eprintln!("ファイルの保存に失敗: {}", e);
                } else {
                    println!("ファイルを保存しました: {:?}", save_path);

                    // 成功応答の送信
                    let response = "OK".as_bytes();
                    if let Err(e) = socket.write_all(response).await {
                        eprintln!("応答の送信に失敗: {}", e);
                    }
                }
            } else {
                eprintln!("保存先が選択されていません");

                // エラー応答の送信
                let response = "ERROR: No save directory selected".as_bytes();
                if let Err(e) = socket.write_all(response).await {
                    eprintln!("エラー応答の送信に失敗: {}", e);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// クライアントモード（ファイル送信）の実装
async fn run_client(server_opt: Option<String>, hotkey_str: &str) -> Result<()> {
    println!("クライアントモード（ファイル送信）を開始します");
    println!("ホットキー: {}", hotkey_str);

    // サーバーアドレスの設定
    let server_addr = if let Some(server) = server_opt {
        format!("{}:{}", server, FILE_TRANSFER_PORT)
    } else {
        format!("localhost:{}", FILE_TRANSFER_PORT)
    };

    println!("サーバーアドレス: {}", server_addr);

    // ホットキーマネージャーの初期化
    let hotkey_manager = GlobalHotKeyManager::new().unwrap();
    let hotkey = parse_hotkey(hotkey_str)?;
    hotkey_manager.register(hotkey).unwrap();

    // ホットキーイベントの監視
    let hotkey_channel = GlobalHotKeyEvent::receiver();

    println!("ファイル転送クライアントを起動しました");
    println!("ホットキー {} を押すとファイルを選択できます", hotkey_str);

    // メインループ
    loop {
        // ホットキーイベントの確認
        if let Ok(event) = hotkey_channel.try_recv() {
            if event.id == hotkey.id() {
                println!("ホットキーが押されました");

                // ファイルの選択
                if let Some(path) = FileDialog::new()
                    .set_title("送信するファイルを選択")
                    .pick_file()
                {
                    println!("ファイルを選択: {:?}", path);

                    // ファイル転送の実行
                    if let Err(e) = send_file(&server_addr, &path).await {
                        eprintln!("ファイル転送に失敗: {}", e);
                    }
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// ファイル送信関数
async fn send_file(server_addr: &str, file_path: &PathBuf) -> Result<()> {
    println!("ファイル転送を開始: {:?}", file_path);

    // サーバーに接続
    let mut socket = TcpStream::connect(server_addr).await?;
    println!("サーバーに接続しました");

    // ファイル名の取得
    let filename = file_path
        .file_name()
        .context("ファイル名の取得に失敗")?
        .to_string_lossy()
        .into_owned();

    // ファイルデータの読み込み
    let filedata = fs::read(file_path)?;

    // ファイル名の長さを送信
    let filename_len = filename.len() as u32;
    socket.write_all(&filename_len.to_be_bytes()).await?;

    // ファイルデータの長さを送信
    let filedata_len = filedata.len() as u32;
    socket.write_all(&filedata_len.to_be_bytes()).await?;

    // ファイル名を送信
    socket.write_all(filename.as_bytes()).await?;
    println!("ファイル名を送信: {}", filename);

    // ファイルデータを送信
    socket.write_all(&filedata).await?;
    println!("ファイルデータを送信: {} バイト", filedata.len());

    // 応答の受信
    let mut response = [0u8; 1024];
    let n = socket.read(&mut response).await?;
    let response_str = String::from_utf8_lossy(&response[..n]);
    println!("サーバーからの応答: {}", response_str);

    println!("ファイル転送が完了しました");

    Ok(())
}

// 対話的にモードを選択する関数
async fn interactive_mode() -> Result<()> {
    println!("ファイル転送プログラム");
    println!("=====================");
    println!("1. サーバーモード（ファイル受信）");
    println!("2. クライアントモード（ファイル送信）");
    println!("選択してください (1/2): ");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    match input.trim() {
        "1" => {
            println!("サーバーモードを選択しました");
            println!("ホットキー: ctrl+shift+r");
            run_server("ctrl+shift+r").await?;
        }
        "2" => {
            println!("クライアントモードを選択しました");
            println!("サーバーのIPアドレスを入力してください: ");

            let mut server_ip = String::new();
            std::io::stdin().read_line(&mut server_ip)?;
            let server_ip = server_ip.trim().to_string();

            if server_ip.is_empty() {
                println!("IPアドレスが入力されていません。localhostを使用します。");
                run_client(None, "ctrl+shift+s").await?;
            } else {
                println!("サーバーIPアドレス: {}", server_ip);
                run_client(Some(server_ip), "ctrl+shift+s").await?;
            }
        }
        _ => {
            println!("無効な選択です。プログラムを終了します。");
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // コマンドライン引数の確認
    let args: Vec<String> = std::env::args().collect();

    if args.len() <= 1 {
        // 引数がない場合は対話モード
        interactive_mode().await?;
    } else {
        // 引数がある場合は通常のCLIモード
        let cli = Cli::parse();

        match &cli.command {
            Commands::Server { hotkey } => {
                run_server(hotkey).await?;
            }
            Commands::Client { server, hotkey } => {
                run_client(server.clone(), hotkey).await?;
            }
        }
    }

    Ok(())
}

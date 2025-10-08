use axum::{
    extract::{ws::WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use chrono::Local;
use clap::Parser;
use futures_util::StreamExt;
use rand::{distributions::Alphanumeric, Rng};
use std::{
    fs::{create_dir_all, File},
    io::Write,
    path::PathBuf,
};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

#[derive(Parser)]
#[command(name = "ws_test_server")]
#[command(about = "WebSocket test server for audio fork testing")]
struct Args {
    #[arg(short, long, default_value = "8080")]
    port: u16,

    #[arg(short, long)]
    dir: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    create_dir_all(&args.dir).expect("Failed to create output directory");

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .layer(ServiceBuilder::new().layer(CorsLayer::permissive()))
        .with_state(args.dir.clone());

    let addr = format!("0.0.0.0:{}", args.port);
    println!("WebSocket server listening on {}", addr);
    println!("Writing binary frames to directory: {}", args.dir.display());

    let listener = TcpListener::bind(&addr).await.expect("Failed to bind");
    axum::serve(listener, app).await.expect("Server failed");
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(dir): axum::extract::State<PathBuf>,
) -> Response {
    println!("new connection");
    ws.on_upgrade(|socket| handle_websocket(socket, dir))
}

async fn handle_websocket(socket: WebSocket, dir: PathBuf) {
    let mut receiver = socket;

    let now = Local::now();
    let random_chars: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    
    let filename = format!(
        "{}_{}",
        now.format("%Y%m%d_%H%M%S"),
        random_chars
    );
    
    let file_path = dir.join(filename);
    
    let mut file = match File::create(&file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create file {}: {}", file_path.display(), e);
            return;
        }
    };

    println!("WebSocket connection established, writing to: {}", file_path.display());

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                println!("Received {} bytes of binary data", data.len());
                
                if let Err(e) = file.write_all(&data) {
                    eprintln!("Error writing to file: {}", e);
                    break;
                }
                if let Err(e) = file.flush() {
                    eprintln!("Error flushing file: {}", e);
                    break;
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                println!("WebSocket connection closed");
                break;
            }
            Ok(_) => {
                // Ignore text messages and other frame types
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        }
    }

    println!("WebSocket handler finished for file: {}", file_path.display());
}

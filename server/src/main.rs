use futures_util::lock::Mutex;
use futures_util::stream::SplitSink;
use futures_util::SinkExt;
use futures_util::StreamExt;
use opencv::core::Mat;
use opencv::core::Rect;
use opencv::core::Size;
use opencv::core::Vector;
use opencv::imgcodecs::IMWRITE_JPEG_QUALITY;
use opencv::imgproc;
use opencv::objdetect::CascadeClassifier;
use opencv::prelude::VectorToVec;
use opencv::prelude::*;
use opencv::videoio::VideoCapture;
use opencv::videoio::CAP_ANY;
use std::env;
use std::sync::Arc;
use tokio::sync::mpsc;
use warp::ws::Message;
use warp::ws::WebSocket;
use warp::Filter;

fn get_absolute_project_path() -> Option<String> {
    match env::current_dir() {
        Ok(path) => path.to_str().map(|s| s.to_string()),
        Err(_) => None,
    }
}

fn capture_video(tx: mpsc::UnboundedSender<(Vec<u8>, usize)>) {
    let mut cam = match VideoCapture::new(0, CAP_ANY) {
        Ok(cam) => {
            VideoCapture::is_opened(&cam).expect("Unable to open default camera!");
            cam
        }
        Err(e) => {
            eprintln!("Failed to open default camera: {}", e);
            return;
        }
    };

    let mut frame = Mat::default();
    let params = vec![IMWRITE_JPEG_QUALITY, 30];

    let absolute_project_path = match get_absolute_project_path() {
        Some(path) => path,
        None => {
            eprintln!("Failed to get absolute project path");
            return;
        }
    };

    println!("Absolute project path: {}", absolute_project_path);

    let mut face_cascade = match CascadeClassifier::new(
        &(absolute_project_path + "/model/haarcascade_frontalface_default.xml"),
    ) {
        Ok(cascade) => cascade,
        Err(e) => {
            eprintln!("Failed to load face cascade: {}", e);
            return;
        }
    };

    loop {
        match cam.read(&mut frame) {
            Ok(_) => {
                if frame.size().unwrap().width > 0 {
                    let mut faces = Vector::<Rect>::new();
                    let mut gray = Mat::default();
                    imgproc::cvt_color(&frame, &mut gray, imgproc::COLOR_BGR2GRAY, 0).unwrap();

                    face_cascade
                        .detect_multi_scale(
                            &gray,
                            &mut faces,
                            1.2,
                            5,
                            0,
                            Size::new(30, 30),
                            Size::new(0, 0),
                        )
                        .unwrap();

                    // 검출된 얼굴 주위에 사각형을 그립니다.
                    for face in faces.iter() {
                        imgproc::rectangle(
                            &mut frame,
                            face,
                            opencv::core::Scalar::new(0.0, 255.0, 0.0, 0.0),
                            2,
                            imgproc::LINE_8,
                            0,
                        )
                        .unwrap();
                    }

                    let mut buf = opencv::core::Vector::<u8>::new();
                    let params = opencv::core::Vector::<i32>::from(params.clone());

                    opencv::imgcodecs::imencode(".jpg", &frame, &mut buf, &params).unwrap();

                    println!(
                        "Captured frame: {} bytes, faces detected: {}",
                        buf.len(),
                        faces.len()
                    );

                    if tx.send((buf.to_vec(), faces.len())).is_err() {
                        eprintln!("Failed to send frame over WebSocket");
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(66)); // 15fps 전송 주기
            }
            Err(e) => {
                eprintln!("Failed to read frame: {}", e);
                continue;
            }
        }
    }
}

async fn send_frame(
    ws_tx: &Arc<Mutex<SplitSink<WebSocket, Message>>>,
    message: Message,
) -> Result<(), ()> {
    ws_tx.lock().await.send(message).await.map_err(|_| ())
}

async fn send_frames(
    ws_tx: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    mut rx: mpsc::UnboundedReceiver<(Vec<u8>, usize)>,
) {
    while let Some((camera_frame, face_count)) = rx.recv().await {
        if let Err(_) = send_frame(&ws_tx, Message::binary(camera_frame)).await {
            eprintln!("Failed to send frame over WebSocket");
            break;
        }

        if let Err(_) = send_frame(&ws_tx, Message::text(face_count.to_string())).await {
            eprintln!("Failed to send face count over WebSocket");
            break;
        }
    }
}

async fn handle_message(ws_tx: Arc<Mutex<SplitSink<WebSocket, Message>>>, msg: Message) {
    if let Ok(text) = msg.to_str() {
        println!("Received message: {}", text);

        if let Err(_) = send_frame(&ws_tx, Message::text("Hello, WebSocket!")).await {
            eprintln!("Failed to send message back to the client");
        }
    }
}

async fn handle_websocket(ws: WebSocket, rx: mpsc::UnboundedReceiver<(Vec<u8>, usize)>) {
    let (ws_tx, mut ws_rx) = ws.split();
    let ws_tx = Arc::new(Mutex::new(ws_tx));

    tokio::spawn(send_frames(ws_tx.clone(), rx));

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => handle_message(ws_tx.clone(), msg).await,
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        }
    }
    println!("WebSocket connection closed.");
}

#[tokio::main]
async fn main() {
    let websocket_route = warp::path("ws").and(warp::ws()).map(|ws: warp::ws::Ws| {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::task::spawn_blocking(move || capture_video(tx));
        ws.on_upgrade(move |socket| handle_websocket(socket, rx))
    });

    warp::serve(websocket_route).run(([0, 0, 0, 0], 8080)).await;
}

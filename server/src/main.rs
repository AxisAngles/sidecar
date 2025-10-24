use std::{path::PathBuf, sync::Arc};
use axum::{response::IntoResponse, Json};
use tokio::sync::{mpsc, Mutex};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;

enum Error {
	NoNewlineToSeparatePath,
	InvalidPath,
	IO(std::io::Error),
}

impl axum::response::IntoResponse for Error {
	fn into_response(self) -> axum::response::Response {
		use axum::http::StatusCode;
		use axum::response::Response;
		match self {
			Error::NoNewlineToSeparatePath => {
				(StatusCode::INTERNAL_SERVER_ERROR, "NoNewlineToSeparatePath").into_response()
			}
			Error::InvalidPath => Response::builder()
				.status(500)
				.body("InvalidPath".into())
				.unwrap(),
			Error::IO(io_error) => Response::builder()
				.status(500)
				.body(io_error.to_string().into())
				.unwrap(),
		}
	}
}

// path = game/whatever/code.luau
async fn write_file(body: axum::body::Bytes) -> Result<(), Error> {
	let body = body.as_ref();
	let path_position = body
		.iter()
		.position(|c| *c == b'\n')
		.ok_or(Error::NoNewlineToSeparatePath)?;

	let relative_path = &body[0..path_position];
	let code = &body[path_position + 1..];

	let relative_path_str = std::str::from_utf8(relative_path).map_err(|_| Error::InvalidPath)?;

	let mut file_path = std::env::current_dir().map_err(Error::IO)?;
	file_path.push(relative_path_str);
	let mut dir_path = file_path.clone();
	dir_path.pop();

	// guaranteeFolderPath(path)
	tokio::fs::create_dir_all(dir_path).await.map_err(Error::IO)?;
	// create the file
	tokio::fs::write(file_path, code).await.map_err(Error::IO)?;

	Ok(())
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum Event {
	Create {
		path: PathBuf,
		source: Vec<u8>,
	},
	Update {
		path: PathBuf,
		source: Vec<u8>,
	},
	Delete {
		path: PathBuf,
	},
}

#[derive(Serialize)]
struct Events {
	events: Vec<Event>,
}

impl IntoResponse for Events {
	fn into_response(self) -> axum::response::Response {
		Json(self).into_response()
	}
}

// Shared channel between watcher and poll handler
type SharedReceiver = Arc<Mutex<mpsc::Receiver<Event>>>;

// global static mutable would be ugly; assume we’ll inject it later
static WATCH_PATH: &str = "./game"; // or whatever you like

async fn long_poll(state: axum::extract::State<SharedReceiver>) -> Result<Events, Error> {
	let mut rx = state.lock().await;
	let mut events = Vec::new();

	// block until at least one event arrives
	if let Some(first) = rx.recv().await {
		events.push(first);
		// grab any immediately available ones too
		while let Ok(ev) = rx.try_recv() {
			events.push(ev);
		}
	}

	Ok(Events { events })
}

fn spawn_watcher(tx: mpsc::Sender<Event>) -> notify::Result<RecommendedWatcher> {
	let mut watcher = notify::recommended_watcher(move |res| {
		let tx = tx.clone();
		tokio::spawn(async move {
			match res {
				Ok(event) => {
					use notify::EventKind::*;
					for path in event.paths {
						match &event.kind {
							Create(_) => {
								let src = tokio::fs::read(&path).await.unwrap_or_default();
								let _ = tx.send(Event::Create { path, source: src }).await;
							}
							Modify(_) => {
								let src = tokio::fs::read(&path).await.unwrap_or_default();
								let _ = tx.send(Event::Update { path, source: src }).await;
							}
							Remove(_) => {
								let _ = tx.send(Event::Delete { path }).await;
							}
							_ => {}
						}
					}
				}
				Err(e) => eprintln!("watch error: {:?}", e),
			}
		});
	})?;

	watcher.watch(WATCH_PATH.as_ref(), RecursiveMode::Recursive)?;
	Ok(watcher)
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
	use axum::routing::{get, post};
	use axum::Router;

	println!("Starting Sidecar");

	let (tx, rx) = mpsc::channel(100);
	let shared_rx = Arc::new(Mutex::new(rx));

	// start file watcher
	let _watcher = spawn_watcher(tx).expect("Failed to start watcher");

	let app = Router::new()
		.route("/write_file", post(write_file))
		.route("/poll", get(long_poll))
		.with_state(shared_rx);

	let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
	let listener = tokio::net::TcpListener::bind(addr).await?;
	println!("Listening on http://{addr}");

	axum::serve(listener, app).await?;
	Ok(())
}

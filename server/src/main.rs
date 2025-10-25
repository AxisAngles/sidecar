use std::sync::Arc;
use notify::Watcher;
use tokio::sync::Mutex;
use axum::response::Response;

use axum::extract::State;

enum Error {
	NoNewlineToSeparatePath,
	InvalidPath,
	IO(std::io::Error),
}

impl axum::response::IntoResponse for Error {
	fn into_response(self) -> Response {
		use axum::http::StatusCode;
		match self{
			Error::NoNewlineToSeparatePath=>(StatusCode::INTERNAL_SERVER_ERROR, "NoNewlineToSeparatePath").into_response(),
			Error::InvalidPath=>Response::builder().status(500).body("InvalidPath".into()).unwrap(),
			Error::IO(io_error)=>Response::builder().status(500).body(io_error.to_string().into()).unwrap(),
		}
	}
}

// path = game/whatever/code.luau
async fn write_file(
	State(state):State<Arc<Mutex<WatcherState>>>,
	body: axum::body::Bytes,
) -> Result<(), Error> {
	let body = body.as_ref();
	let path_position = body
		.iter()
		.position(|c|*c==b'\n')
		.ok_or(Error::NoNewlineToSeparatePath)?;

	let relative_path = &body[0 .. path_position];
	let code = &body[path_position + 1 ..];

	let relative_path_str=str::from_utf8(relative_path).map_err(|_|Error::InvalidPath)?;

	let mut file_path = std::env::current_dir().map_err(Error::IO)?;
	file_path.push(relative_path_str);
	let mut dir_path = file_path.clone();
	dir_path.pop();

	// guaranteeFolderPath(path)
	tokio::fs::create_dir_all(dir_path).await.map_err(Error::IO)?;
	// create the file
	tokio::fs::write(&file_path, code).await.map_err(Error::IO)?;

	state.lock().await.watcher.watch(&file_path, notify::RecursiveMode::NonRecursive).unwrap();

	Ok(())
}

#[derive(Debug)]
struct Events{
	events:Vec<notify::Event>,
}

impl axum::response::IntoResponse for Events {
	fn into_response(self) -> Response {
		// just yeet the debug info back to the client
		format!("{self:?}").into_response()
	}
}

// watcher already spawns a thread
// use mpsc sync channel
// move sender into event handler
// send event into channel on file change
struct ReceiverState{
	rx:tokio::sync::mpsc::UnboundedReceiver<notify::Result<notify::Event>>,
}
struct WatcherState{
	watcher:notify::RecommendedWatcher,
}
#[derive(Debug)]
enum PollError{
	Notify(notify::Error),
}
impl From<notify::Error> for PollError{
	fn from(value:notify::Error)->Self{
		PollError::Notify(value)
	}
}
impl axum::response::IntoResponse for PollError{
	fn into_response(self) -> axum::response::Response {
		match self{
			PollError::Notify(e)=>Response::builder().status(500).body(e.to_string().into()).unwrap(),
		}
	}
}
async fn long_poll(State(state):State<Arc<Mutex<ReceiverState>>>)->Result<Events,PollError>{
	// use mpsc sync channel
	// put receiver in application state
	// arc mutex application state
	// begin long poll request
	// lock application state
	// revc from sync channel (tokio blocking thread)
	// reply to long poll
	let rx=&mut state.lock().await.rx;
	let mut events=Vec::new();
	// wait to receive at least one event
	if let Some(first)=rx.recv().await{
		events.push(first?);
	}
	// add additional events
	while let Ok(additional)=rx.try_recv(){
		events.push(additional?);
	}
	Ok(Events{events})
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error>{
	use axum::routing::{get, post};
	println!("Starting Sidecar");

	let (sx,rx)=tokio::sync::mpsc::unbounded_channel();
	let watcher=notify::recommended_watcher(move|event|sx.send(event).unwrap()).unwrap();

	let addr=std::net::SocketAddr::from(([127,0,0,1], 8080));
	let listener=tokio::net::TcpListener::bind(addr).await?;

	let app=axum::Router::new()
		.route("/write_file", post(write_file))
		.route("/write_file", get("heyo"))
		.with_state(Arc::new(Mutex::new(WatcherState{watcher})))
		.route("/poll",get(long_poll))
		.with_state(Arc::new(Mutex::new(ReceiverState{rx})));

	axum::serve(listener, app).await?;

	Ok(())
}

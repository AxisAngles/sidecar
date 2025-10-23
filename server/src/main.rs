use std::path::PathBuf;

enum Error {
	NoNewlineToSeparatePath,
	InvalidPath,
	IO(std::io::Error),
}

impl axum::response::IntoResponse for Error {
	fn into_response(self) -> axum::response::Response {
		use axum::http::StatusCode;
		use axum::response::Response;
		match self{
			Error::NoNewlineToSeparatePath=>(StatusCode::INTERNAL_SERVER_ERROR, "NoNewlineToSeparatePath").into_response(),
			Error::InvalidPath=>Response::builder().status(500).body("InvalidPath".into()).unwrap(),
			Error::IO(io_error)=>Response::builder().status(500).body(io_error.to_string().into()).unwrap(),
		}
	}
}

// path = game/whatever/code.luau
async fn write_file(body: axum::body::Bytes) -> Result<(), Error> {
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
	tokio::fs::write(file_path, code).await.map_err(Error::IO)?;

	Ok(())
}

enum Event{
	Create{
		path:PathBuf,
		source:Vec<u8>,
	},
	Update{
		path:PathBuf,
		source:Vec<u8>,
	},
	Delete{
		path:PathBuf,
	},
}
struct Events{
	events:Vec<Event>,
}

impl axum::response::IntoResponse for Events {
	fn into_response(self) -> axum::response::Response {
		unimplemented!()
	}
}

// watcher already spawns a thread
// use mpsc sync channel
// move sender into event handler
// send event into channel on file change

async fn poll()->Result<Events,Error>{
	notify::recommended_watcher(event_handler);
	// use mpsc sync channel
	// put receiver in application state
	// arc mutex application state
	// begin long poll request
	// lock application state
	// revc from sync channel (tokio blocking thread)
	// reply to long poll
	unimplemented!()
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error>{
	use axum::routing::{get, post};
	println!("Starting Sidecar");

	let addr=std::net::SocketAddr::from(([127,0,0,1], 8080));
	let listener=tokio::net::TcpListener::bind(addr).await?;

	let app=axum::Router::new()
		.route("/write_file", post(write_file))
		.route("/write_file", get("heyo"))
		.route("/poll",get(poll));

	axum::serve(listener, app).await?;

	Ok(())
}

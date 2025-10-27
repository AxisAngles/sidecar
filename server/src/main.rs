use std::net::TcpListener;
use std::thread::spawn;
use tungstenite::accept;
use tungstenite::Bytes;

#[derive(Debug)]
enum Error {
	NoNewlineToSeparatePath,
	InvalidPath,
	IO(std::io::Error),
}

// path = game/whatever/code.luau
fn write_file(body: &[u8]) -> Result<(), Error> {
	//let body = body.as_ref();
	let path_position = body
		.iter()
		.position(|c| *c == b'\n')
		.ok_or(Error::NoNewlineToSeparatePath)?;

	let relative_path = &body[0..path_position];
	let code = &body[path_position + 1..];

	let relative_path_str = str::from_utf8(relative_path).map_err(|_| Error::InvalidPath)?;

	let mut file_path = std::env::current_dir().map_err(Error::IO)?;
	file_path.push(relative_path_str);
	let mut dir_path = file_path.clone();
	dir_path.pop();

	// guaranteeFolderPath(path)
	std::fs::create_dir_all(dir_path).map_err(Error::IO)?;
	// create the file
	std::fs::write(file_path, code).map_err(Error::IO)?;

	Ok(())
}

fn main() {
	let listener = TcpListener::bind("127.0.0.1:8080").unwrap();
	let mut incoming_iterator = listener.incoming();
	loop {
		let stream_in = incoming_iterator.next().unwrap().unwrap();
		spawn(move || {
			let mut websocket = accept(stream_in).unwrap();
			loop {
				let read_result = websocket.read();
				match read_result {
					Ok(message) => write_file(&message.into_data()).unwrap(),
					Err(_) => break,
				};
			}
		});

		// let stream_out = incoming_iterator.next().unwrap().unwrap();
		// spawn(move || {
		// 	let websocket = accept(stream_out).unwrap();

		// });
	}
}

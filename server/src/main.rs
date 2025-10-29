use notify::Watcher;
use notify::event::CreateKind;
use notify::event::ModifyKind;
use notify::event::RemoveKind;
use std::env::current_dir;
use std::net::TcpListener;
use std::path::Path;
use std::thread::spawn;
use tungstenite::accept;

#[expect(unused)]
#[derive(Debug)]
enum Error {
	NoNewlineToSeparatePath,
	InvalidPath,
	IO(std::io::Error),
}

// path = game/whatever/code.luau
fn write_file(base_dir: &Path, body: &[u8]) -> Result<(), Error> {
	//let body = body.as_ref();
	let path_position = body
		.iter()
		.position(|c| *c == b'\n')
		.ok_or(Error::NoNewlineToSeparatePath)?;

	let relative_path = &body[0..path_position];
	let code = &body[path_position + 1..];

	let relative_path_str = str::from_utf8(relative_path).map_err(|_| Error::InvalidPath)?;

	let mut file_path = base_dir.to_owned();
	file_path.push(relative_path_str);
	let mut dir_path = file_path.clone();
	dir_path.pop();

	// guaranteeFolderPath(path)
	std::fs::create_dir_all(dir_path).map_err(Error::IO)?;
	// create the file
	std::fs::write(file_path, code).map_err(Error::IO)?;

	Ok(())
}

fn run_server(base_dir: &Path) {
	let listener_to_server = TcpListener::bind("127.0.0.1:8080").unwrap();
	let listener_to_plugin = TcpListener::bind("127.0.0.1:8081").unwrap();
	let mut incoming_iterator_to_server = listener_to_server.incoming();
	let mut incoming_iterator_to_plugin = listener_to_plugin.incoming();

	loop {
		let (sender, receiver) = std::sync::mpsc::channel();
		let mut watcher = notify::recommended_watcher(move |result| {
			sender.send(result).unwrap();
		})
		.unwrap();
		watcher
			.watch(base_dir, notify::RecursiveMode::Recursive)
			.unwrap();

		let stream_to_server = incoming_iterator_to_server.next().unwrap().unwrap();
		let base_dir = base_dir.to_owned();
		let to_server_join_handle = spawn(move || {
			let mut websocket = accept(stream_to_server).unwrap();
			loop {
				let read_result = websocket.read();
				match read_result {
					Ok(message) => write_file(&base_dir, &message.into_data()).unwrap(),
					Err(_) => break,
				};
			}
		});

		let stream_to_plugin = incoming_iterator_to_plugin.next().unwrap().unwrap();
		let to_plugin_join_handle = spawn(move || {
			// move watcher into this thread, otherwise it gets dropped at the end of the containing loop
			// let _ = watcher, does not actually capture the watcher, btw
			let _watcher = watcher;
			let mut websocket = accept(stream_to_plugin).unwrap();
			for event_result in receiver.iter() {
				let event = event_result.unwrap();
				println!("{event:?}");
				match event.kind {
					notify::EventKind::Create(CreateKind::File) | notify::EventKind::Create(CreateKind::Any) => {
						for path in event.paths {
							let mut message = Vec::new();
							message.push(b'c'); // c is create
							message.extend_from_slice(path.as_os_str().as_encoded_bytes());
							message.push(b'\n');
							message.extend_from_slice(&std::fs::read(path).unwrap());
							websocket.send(message.into()).unwrap();
						}
					}
					notify::EventKind::Modify(ModifyKind::Data(_)) | notify::EventKind::Modify(ModifyKind::Any) => {
						for path in event.paths {
							let mut message = Vec::new();
							message.push(b'u'); // u is update
							message.extend_from_slice(path.as_os_str().as_encoded_bytes());
							message.push(b'\n');
							message.extend_from_slice(&std::fs::read(path).unwrap());
							websocket.send(message.into()).unwrap();
						}
					}
					notify::EventKind::Remove(RemoveKind::File) | notify::EventKind::Remove(RemoveKind::Any) => {
						for path in event.paths {
							let mut message = Vec::new();
							message.push(b'd'); // d is delete
							message.extend_from_slice(path.as_os_str().as_encoded_bytes());
							websocket.send(message.into()).unwrap();
						}
					}
					_ => {}
				}
			}
		});

		to_server_join_handle.join().unwrap();
		to_plugin_join_handle.join().unwrap();
	}
}

fn main() {
	run_server(&current_dir().unwrap());
}

#[cfg(test)]
mod test {
	use super::*;
	#[test]
	fn test_create_update_delete() {
		// set up paths
		let fname = "test.luau";
		let mut cd = current_dir().unwrap();
		cd.push("test");
		let mut fpath = cd.clone();
		fpath.push(fname);

		// clean up from previous tests
		_ = std::fs::remove_file(&fpath);

		// run server
		let base_dir = cd.clone();
		spawn(move || {
			run_server(&base_dir);
		});
		// let the server start up (todo: gracefully detect startup completion)
		std::thread::sleep(std::time::Duration::from_millis(5));

		// connect to server
		let (mut c_write, _) = tungstenite::connect("ws://127.0.0.1:8080").unwrap();
		let (mut c_updates, _) = tungstenite::connect("ws://127.0.0.1:8081").unwrap();

		// spawn a thread to monitor the updates because we can't read without blocking the main thread.
		// this is a compatibility layer to allow us to throw away junk events.
		let (send, recv) = std::sync::mpsc::channel();
		spawn(move || {
			loop {
				send.send(c_updates.read().unwrap().into_data()).unwrap();
			}
		});

		// write file
		let fcode = "print(\"test\")";
		let fpath_str = fpath.as_os_str().to_str().unwrap();
		let serialized = format!("{fname}\n{fcode}");
		println!("EXPECT: create + junk events");
		c_write.send(serialized.as_bytes().into()).unwrap();
		// wait for file create event
		let observed_event = recv.recv().unwrap();
		// drop junk
		std::thread::sleep(std::time::Duration::from_millis(5));
		while let Ok(junk) = recv.try_recv() {
			println!("JUNK: {junk:?}");
		}
		let expected_event = format!("c{}\n{fcode}", fpath_str);
		assert_eq!(observed_event, expected_event);
		// assert file exists
		assert_eq!(std::fs::read(&fpath).unwrap(), fcode.as_bytes());

		// update file
		let fcode = "print(\"test2\")";
		println!("EXPECT: update + junk events");
		std::fs::write(&fpath, fcode).unwrap();
		// assert a file update was recieved
		let observed_data = recv.recv().unwrap();
		// drop junk
		std::thread::sleep(std::time::Duration::from_millis(5));
		while let Ok(junk) = recv.try_recv() {
			println!("JUNK: {junk:?}");
		}
		let expected_data = format!("u{}\n{fcode}", fpath_str);
		assert_eq!(observed_data, expected_data);

		// delete file
		println!("EXPECT: remove + junk events");
		std::fs::remove_file(&fpath).unwrap();
		// assert a file remove was recieved
		let observed_data = recv.recv().unwrap();
		// drop junk
		std::thread::sleep(std::time::Duration::from_millis(5));
		while let Ok(junk) = recv.try_recv() {
			println!("JUNK: {junk:?}");
		}
		let expected_data = format!("d{}", fpath_str);
		assert_eq!(observed_data, expected_data);

		// close server (how?)
	}
}

use futures_util::SinkExt;
use notify::Watcher;
use notify::event::CreateKind;
use notify::event::ModifyKind;
use notify::event::RemoveKind;
use tokio_tungstenite::accept_async;
use std::env::current_dir;
use tokio::net::TcpListener;
use std::path::Path;
use futures_util::StreamExt;

#[expect(unused)]
#[derive(Debug)]
enum Error {
	NoNewlineToSeparatePath,
	InvalidPath,
	IO(std::io::Error),
}

// path = game/whatever/code.luau
async fn write_file(base_dir: &Path, body: &[u8]) -> Result<(), Error> {
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
	tokio::fs::create_dir_all(dir_path).await.map_err(Error::IO)?;
	// create the file
	tokio::fs::write(file_path, code).await.map_err(Error::IO)?;

	Ok(())
}

async fn run_server(base_dir: &Path) {
	let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

	let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
	let mut watcher = notify::recommended_watcher(move |result| {
		sender.send(result).unwrap();
	}).unwrap();

	watcher.watch(base_dir, notify::RecursiveMode::Recursive).unwrap();

	while let Ok((stream, addr)) = listener.accept().await {
		println!("connecting to addr {addr:?}");
		let mut websocket_stream = accept_async(stream).await.unwrap();

		// what we want to write
		loop {
			tokio::select! {
				event_result_option = receiver.recv() => match event_result_option {
					Some(Ok(notify::Event{kind : notify::EventKind::Create(CreateKind::File) | notify::EventKind::Create(CreateKind::Any), paths, attrs:_})) => {
						for path in paths {
							let mut message = Vec::new();
							message.push(b'c'); // c is create
							message.extend_from_slice(path.as_os_str().as_encoded_bytes());
							message.push(b'\n');
							message.extend_from_slice(&tokio::fs::read(path).await.unwrap());
							websocket_stream.send(message.into()).await.unwrap();
						}
					}
					Some(Ok(notify::Event{kind : notify::EventKind::Modify(ModifyKind::Data(_)) | notify::EventKind::Modify(ModifyKind::Any), paths, attrs:_})) => {
						for path in paths {
							let mut message = Vec::new();
							message.push(b'u'); // u is update
							message.extend_from_slice(path.as_os_str().as_encoded_bytes());
							message.push(b'\n');
							message.extend_from_slice(&tokio::fs::read(path).await.unwrap());
							websocket_stream.send(message.into()).await.unwrap();
						}
					}
					Some(Ok(notify::Event{kind : notify::EventKind::Remove(RemoveKind::File) | notify::EventKind::Remove(RemoveKind::Any), paths, attrs:_})) => {
						for path in paths {
							let mut message = Vec::new();
							message.push(b'd'); // d is delete
							message.extend_from_slice(path.as_os_str().as_encoded_bytes());
							websocket_stream.send(message.into()).await.unwrap();
						}
					}
					Some(result) => {result.unwrap();},
					None => break,
				},
				message_result_option = websocket_stream.next() => match message_result_option {
					Some(Ok(message)) => {write_file(base_dir, &message.into_data()).await.unwrap();},
					Some(result) => {result.unwrap();},
					None => break,
				}
			}
		}
    };
}

#[tokio::main]
async fn main() {
	run_server(&current_dir().unwrap()).await;
}

// NEXT UP:
// race two futures
// file watcher outputting an event vs websocket outputting an event
// whichever one wins, you do somthing
// if file watcher does something, it sends, otherwise it resets the loop
// 2 threads, file watcher spawns a thread no matter what.
// receive the events on the main thread through a sync channel
// main thread is responsible for receiving from the websocket
// use select to run code on whatever thread fires off first

#[cfg(test)]
mod test {
	use super::*;
	use tokio::spawn;
	#[tokio::test]
	async fn test_create_update_delete() {
		// set up paths
		let fname = "test.luau";
		let mut cd = current_dir().unwrap();
		cd.push("test");
		let mut fpath = cd.clone();
		fpath.push(fname);

		// clean up from previous tests
		_ = tokio::fs::remove_file(&fpath).await;

		// run server
		let base_dir = cd.clone();
		let server = spawn(async move {
			run_server(&base_dir).await;
		});

		// let the server start up (todo: gracefully detect startup completion)
		tokio::time::sleep(std::time::Duration::from_millis(5)).await;

		let client = spawn(async move{
			// connect to server
			let (mut connection, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:8080").await.unwrap();

			macro_rules! drop_junk {
				() => {
					// drop junk for up to 5ms
					tokio::select! {
						_ = async {
							while let Some(junk) = connection.next().await {
								println!("JUNK: {junk:?}");
							}
						} => {}
						//tokio::time
						_ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {}
					}
				};
			}

			// write file
			let fcode = "print(\"test\")";
			let fpath_str = fpath.as_os_str().to_str().unwrap();
			let serialized = format!("{fname}\n{fcode}");
			println!("EXPECT: create + junk events");
			connection.send(serialized.as_bytes().into()).await.unwrap();
			// wait for file create event
			let observed_event = connection.next().await.unwrap().unwrap().into_data();
			let expected_event = format!("c{}\n{fcode}", fpath_str);
			assert_eq!(observed_event, expected_event);
			// assert file exists
			assert_eq!(tokio::fs::read(&fpath).await.unwrap(), fcode.as_bytes());
			drop_junk!();

			// update file
			let fcode = "print(\"test2\")";
			println!("EXPECT: update + junk events");
			tokio::fs::write(&fpath, fcode).await.unwrap();
			// assert a file update was recieved
			let observed_data = connection.next().await.unwrap().unwrap().into_data();
			let expected_data = format!("u{}\n{fcode}", fpath_str);
			assert_eq!(observed_data, expected_data);
			drop_junk!();

			// delete file
			println!("EXPECT: remove + junk events");
			tokio::fs::remove_file(&fpath).await.unwrap();
			// assert a file remove was recieved
			let observed_data = connection.next().await.unwrap().unwrap().into_data();
			let expected_data = format!("d{}", fpath_str);
			assert_eq!(observed_data, expected_data);
			drop_junk!();

			// close connection
			connection.close(None).await.unwrap();
		});

		// graceful shutdown (server shuts down when connection is closed)
		client.await.unwrap();
		server.await.unwrap();
	}
}

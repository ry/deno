#![allow(dead_code)]
extern crate futures;
extern crate tokio;
extern crate websocket;

use std::fmt::Debug;
use websocket::r#async::Server;
// use websocket::async::Server;
use futures::{future, Future, Sink, Stream};
use std::sync::mpsc::channel;
use std::thread;
use tokio::runtime::TaskExecutor;
use websocket::message::{Message, OwnedMessage};
use websocket::server::InvalidConnection;

const ADDR: &'static str = "0.0.0.0:9229";

pub fn spawn() {
  server();

  /*
  // let (sender, receiver) = channel();
  // Create a new runtime to evaluate the future asynchronously.
  thread::spawn(move || {
    // let mut rt = create_threadpool_runtime();
    // sender.send(r).unwrap();
  });
  // receiver.recv().unwrap()
*/
}

fn server() {
  let mut runtime = tokio::runtime::Builder::new().build().unwrap();
  let executor = runtime.executor();
  // bind to the server
  let server = Server::bind(ADDR, &tokio::reactor::Handle::default()).unwrap();

  // time to build the server's future
  // this will be a struct containing everything the server is going to do

  println!("WebSocket server {}", ADDR);
  println!("Go to chrome://inspect");

  // a stream of incoming connections
  let f = server
		.incoming()
    .map(|socket| {
			println!("new connection");
			socket
		})
		.then(future::ok)
		.filter(|event| {
			match event {
				Ok(_) => true, // a good connection
				Err(InvalidConnection { ref error, .. }) => {
					println!("Bad client: {}", error);
					false // we want to save the stream if a client cannot make a valid handshake
				}
			}
		})
		.and_then(|event| event) // unwrap good connections
		.map_err(|_| ()) // and silently ignore errors (in `.filter`)
		.for_each(move |(upgrade, addr)| {
			println!("Got a connection from: {}", addr);
			// check if it has the protocol we want

			/*
			if !upgrade.protocols().iter().any(|s| s == "rust-websocket") {
				// reject it if it doesn't
				spawn_future(upgrade.reject(), "Upgrade Rejection", &executor);
				return Ok(());
			}
			*/

			// accept the request to be a ws connection if it does
			let f = upgrade
				.use_protocol("rust-websocket")
				.accept()
				// send a greeting!
				.and_then(|(s, _)| s.send(Message::text("Hello World!").into()))
				// simple echo server impl
				.and_then(|s| {
					let (sink, stream) = s.split();
					stream
						.take_while(|m| Ok(!m.is_close()))
						.filter_map(|m| {
							println!("Message from Client: {:?}", m);
							match m {
								OwnedMessage::Ping(p) => Some(OwnedMessage::Pong(p)),
								OwnedMessage::Pong(_) => None,
								_ => Some(m),
							}
						})
						.forward(sink)
						.and_then(|(_, sink)| sink.send(OwnedMessage::Close(None)))
				});

			spawn_future(f, "Client Status", &executor);
			Ok(())
		});

  runtime.block_on(f).unwrap();
}

fn spawn_future<F, I, E>(f: F, desc: &'static str, executor: &TaskExecutor)
where
  F: Future<Item = I, Error = E> + 'static + Send,
  E: Debug,
{
  executor.spawn(
    f.map_err(move |e| println!("{}: '{:?}'", desc, e))
      .map(move |_| println!("{}: Finished.", desc)),
  );
}

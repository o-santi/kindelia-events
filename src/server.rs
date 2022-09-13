use std::io::Error;
use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_tungstenite::tungstenite::Message;

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, StreamExt, stream::TryStreamExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use crate::db::{Database, Post};

pub const GET: u8 = 0;
pub const POST: u8 = 1;
pub const WATCH: u8 = 2;
pub const UNWATCH: u8 = 3;
pub const TIME: u8 = 4;
pub const ERROR: u8 = 7;

async fn handle_connection_websocket(stream: TcpStream, addr: SocketAddr, conn: Arc<Mutex<Database>>) {
    // let addr = stream.peer_addr().expect("connected streams should have peer address set");
    println!("Incoming TCP from : {}", addr);
    let ws_stream = tokio_tungstenite::accept_async(stream)
	.await
	.expect("Error during websocket handshake");

    println!("Websocket connection established with: {}", addr);

    let (tx, rx) = unbounded();

    let (outgoing, incoming) = ws_stream.split();
    
    let broadcast_incoming = incoming.try_for_each(|msg| {
	on_tcp_message(&mut conn.lock().unwrap(), &msg, addr, tx.clone());
	future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    conn.lock()
	.unwrap()
	.watch_list
	.iter_mut()
	.for_each(|(_, peers)| peers.retain(|(peer_addr, _)| *peer_addr != addr));
}

fn parse_get_request(data: &[u8]) -> (u64, u64, u64) {
    let room = u64::from_be_bytes(data[1..9].try_into().expect("error reading room_id"));
    let from_timestamp = u64::from_be_bytes(data[9..17].try_into().expect("error reading from-timestamp"));
    let to_timestamp = u64::from_be_bytes(data[17..25].try_into().expect("error reading to-timestamp"));
    (room, from_timestamp, to_timestamp)
}

fn get_post(conn: &mut Database, data: &[u8], channel: UnboundedSender<Message>) {
    let (room, from_timestamp, to_timestamp) = parse_get_request(data);
    let posts = conn.get_post(room, from_timestamp, to_timestamp);
    match posts {
	Err(err) => send_to_channel(error_msg_data(err), channel),
	Ok(psts) => 
	    for p in psts {
		send_to_channel(post_data(room.clone(), p), channel.clone());
	    }
    };
}

fn post_to_db(conn: &mut Database, data: &[u8], channel: UnboundedSender<Message>) {
    let room = u64::from_be_bytes(data[1..9].try_into().expect("error reading room_id"));
    let data : Vec<u8> = data[9..data.len()].try_into().expect("error reading post data");
    if data.len() > (1024 + 9) {
	send_to_channel(error_msg_data("Cannot post more than 1024 bytes per request".into()), channel);
    }
    else {
	let post = conn.save_post(room, data.to_vec());
	if let Some(watchers) = conn.watch_list.get_mut(&room) {
	    for (_addr, tx) in watchers {
		println!("Sending post to {}", _addr);
		send_to_channel(post_data(room.clone(), post.clone()), tx.clone());
	    }
	}
    }
}


fn on_tcp_message(conn: &mut Database, message: &Message, addr: SocketAddr, channel: UnboundedSender<Message>) {
    let data = match message {
	Message::Text(dat) => dat.as_bytes(),
	Message::Binary(dat) => dat,
	_ => &[],
    };
    if let Some(head) = data.first() {
	match *head {
	    GET =>  { get_post(conn, data, channel); }
	    POST => { post_to_db(conn, data, channel); }
	    WATCH => {
		let room = u64::from_be_bytes(data[1..9].try_into().expect("error reading room id"));
		println!("WATCH from {} for room {}", addr, room);
		conn.watch_room(room, &(addr, channel))
	    }
	    UNWATCH => {
		let room = u64::from_be_bytes(data[1..9].try_into().expect("error reading room id"));
		println!("UNWATCH from {} for room {}", addr, room);
		conn.unwatch_room(room, &(addr, channel));
	    }
	    TIME => {
		println!("TIME from {}", addr);
		send_to_channel(time_data(), channel);
	    }
	    
	    _ => {
		send_to_channel(error_msg_data("Unkown header.".into()), channel);
	    }
	}
    }
}

async fn on_udp_message(conn: Arc<Mutex<Database>>, data: Vec<u8>, socket: Arc<UdpSocket>, addr: SocketAddr) {
    if let Some(&head) = data.first() {
	match head {
	    GET => {
		let (room, from_timestamp, to_timestamp) = parse_get_request(&data);
		let posts = conn.lock().unwrap().get_post(room, from_timestamp, to_timestamp);
		match posts {
		    Err(err) => { socket.send_to(err.as_bytes(), addr).await.unwrap(); },
		    Ok(psts) => 
			for p in psts {
			    let data = post_data(room.clone(), p);
			    socket.send_to(&data, addr).await.unwrap();
			}
		};
	    }
	    POST => {
		if data.len() > (1200 + 9) {
 		    let err_msg = error_msg_data("Cannot post more than 1200 bytes per request".into());
		    socket.send_to(&err_msg, addr).await.unwrap();
		}
		else {
		    let room = u64::from_be_bytes(data[1..9].try_into().expect("error reading room_id"));
		    let data : Vec<u8> = data[9..data.len()].try_into().expect("error reading post data");
		    let mut conn = conn.lock().unwrap();
		    let post = conn.save_post(room, data.to_vec());
		    if let Some(watchers) = conn.watch_list.get_mut(&room){
			for (_addr, tx) in watchers {
			    send_to_channel(post_data(room.clone(), post.clone()), tx.clone());
			}
		    }
		}
	    }
	    TIME => {
		let time = time_data();
		socket.send_to(&time, addr).await.unwrap();
	    }
	    WATCH | UNWATCH => {
		let err = error_msg_data("Watch and Unwatch can only be done through TCP Websockets".into());
		socket.send_to(&err, addr).await.unwrap();
	    }
	    _ => {
		let err = error_msg_data("Unkown code".into());
		socket.send_to(&err, addr).await.unwrap();
	    }
	}
    }
}

fn post_data(room_id: u64, post: Post) -> Vec<u8> {
    let (data, time) = post;
    let header = u8::to_be_bytes(POST).to_vec();
    let stamp = u64::to_be_bytes(time).to_vec();
    let room = u64::to_be_bytes(room_id).to_vec();
    let message = [header, room, stamp, data].concat();
    message
}

fn time_data() -> Vec<u8> {
    let header = u8::to_be_bytes(TIME).to_vec();
    let time = SystemTime::now().duration_since(UNIX_EPOCH).expect("time went backwards").as_millis() as u64;
    let time_vec = u64::to_be_bytes(time).to_vec();
    let message = [header, time_vec].concat();
    message
}

fn error_msg_data(err: String) -> Vec<u8> {
    let header = u8::to_be_bytes(ERROR).to_vec();
    let err_vec = err.as_bytes().to_vec();
    let message = [header, err_vec].concat();
    message
}

fn send_to_channel(data: Vec<u8>, channel: UnboundedSender<Message>) {
    channel
	.unbounded_send(Message::Binary(data))
	.unwrap_or_else(|_| println!("Could not send message to channel"));
}

async fn listen_tcp(port: String, conn: Arc<Mutex<Database>>){
    let tcp_socket = TcpListener::bind(&port).await.unwrap();
    println!("Listening to TCP on port {}", port);

    while let Ok((stream, addr)) = tcp_socket.accept().await {
	tokio::spawn(handle_connection_websocket(stream, addr, conn.clone()));
    }
}

async fn listen_udp(port: String, conn: Arc<Mutex<Database>>) {
    let udp_listener = UdpSocket::bind(&port).await.expect("Could not bind udp server to addr");
    println!("Listening to UDP on port {}", port);
    let sock = Arc::new(udp_listener);
    let mut buf = [0; 65536];

    while let Ok((len, addr)) = sock.recv_from(&mut buf).await {
	println!("Receiving message from {}", addr);
	let msg = &buf[0..len].to_vec();
	tokio::spawn(on_udp_message(conn.clone(), msg.clone(), sock.clone(), addr));
    }
}

#[tokio::main]
pub async fn run() -> Result<(), Error> {
    let tcp_addr = "127.0.0.1:8080";
    let udp_addr = "127.0.0.1:8081";
    
    let conn = Arc::new(Mutex::new(Database::new()));

    tokio::join!(listen_tcp(tcp_addr.into(), conn.clone()), listen_udp(udp_addr.into(), conn));
    
    Ok(())
}

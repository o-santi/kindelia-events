use futures_util::{future, pin_mut, StreamExt};
use futures_channel::mpsc::unbounded;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use crate::server::{WATCH, POST, GET};

#[tokio::main]
pub async fn run() {

    let address = url::Url::parse("ws://127.0.0.1:8080").unwrap();
    let (stdin_tx, stdin_rx) = unbounded();
    
    tokio::spawn(read_stdin(stdin_tx));

    let (ws_stream, _) = connect_async(address).await.expect("could not connect");
    println!("Handshake successful");

    let (write, read) = ws_stream.split();
    let stdin_to_ws = stdin_rx.map(Ok).forward(write);
    let ws_to_stdout = {
        read.for_each(|message| async {
            let data = message.unwrap().into_data();
            tokio::io::stdout().write_all(&data).await.unwrap();
        })
    };
    
    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
    
}

async fn read_stdin(tx: futures_channel::mpsc::UnboundedSender<Message>) {
    let mut stdin = tokio::io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf).await {
            Err(_) | Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);
	let output = parse_input(buf);
        tx.unbounded_send(Message::binary(output)).unwrap();
    }
}

async fn parse_input(buf: &Vec<u8>) -> &Vec<u8> {
    let text = String::from_utf8(buf.clone());
    
    text
}

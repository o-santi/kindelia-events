use std::net::SocketAddr;
use futures_channel::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Message;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub type Peer = (SocketAddr, UnboundedSender<Message>);
pub type Post = (Vec<u8>, u64);

const U60_MASK : u64 = 0xFFFFFFFFFFFFFFF;

#[derive(Clone)]
pub struct Database {
    pub room_posts: HashMap<u64, Vec<Post>>,
    pub watch_list: HashMap<u64, Vec<Peer>>,
}

impl Database {
    pub fn save_post(&mut self, room_id: u64, data: Vec<u8>) -> Post {
	if self.room_posts.get(&room_id).is_none() {
	    self.room_posts.insert(room_id, Vec::new());
	}
	let now = SystemTime::now()
			    .duration_since(UNIX_EPOCH)
			    .expect("time went backwards")
			    .as_millis();
	let post = (data, now as u64);
	self.room_posts
	    .get_mut(&(room_id & U60_MASK))
	    .unwrap()
	    .push(post.clone());
	return post;
    }

    pub fn get_post(&mut self, room_id: u64, from_id: u64, to_id: u64) -> Result<Vec<Post>, String> {
	let room_u60 = room_id & U60_MASK;
	match self.room_posts.get(&room_u60) {
	    None => Err(format!("Room with id {:?} does not exist", room_id)),
	    Some(room) => {
		let mut ret = vec![];
		for (i, (data, tim)) in room.iter().enumerate() {
		    if (i as u64) >= from_id && (i as u64) < to_id {
			ret.push((data.clone(), tim.clone()));
		    }
		}
		// for loops are easier than get mut clone iter stuff
		return Ok(ret)
	    }
	}
    }

    pub fn watch_room(&mut self, room_id: u64, peer:&Peer) {
	let (addr, _) = peer;

	if self.watch_list.get_mut(&(room_id & U60_MASK)).is_none() {
	    self.watch_list.insert(room_id & U60_MASK, Vec::new());
	}

	let watchlist = self.watch_list.get_mut(&room_id).unwrap();

	if !(watchlist.iter().any(|&(watch_addr, _)| *addr == watch_addr)) {
	    watchlist.push(peer.clone())
	}
    }

    pub fn unwatch_room(&mut self, room_id: u64, peer: &Peer) {
	let (peer_addr, _) = peer;
	let watchers = self.watch_list.get_mut(&(room_id & U60_MASK)).unwrap();
	for (i, (watcher_addr, _)) in watchers.iter().enumerate() {
	    if *watcher_addr == *peer_addr {
		watchers.remove(i);
		break;
	    }
	}
    }

    pub fn new() -> Database {
	Database {
	    room_posts: HashMap::new(),
	    watch_list: HashMap::new()
	}
    }

}


mod server;
mod db;
// mod client;

use std::env;


fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| panic!("this program requires a mode to run"));
    match mode.as_str() {
	"--server" => server::run().unwrap(),
	// "--client" => client::run(),
	_ => panic!("unkown mode {mode:?}"),
    };
}


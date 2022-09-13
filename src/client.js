var textEncoding = require("text-encoding");
var utf8_encoder = new TextEncoder("utf-8");
var utf8_decoder = new TextDecoder("utf-8");
var WebSocket = require("isomorphic-ws");

const GET = 0;
const POST = 1;
const WATCH = 2;
const UNWATCH = 3;
const TIME = 4;
const ERROR = 7;

const hex_char = "0123456789abcdef".split("");
function hex_to_bytes(hex) {
    var arr = [];
    for (var i = 0; i < hex.length/2; ++i) {
	arr.push((parseInt(hex[i*2+0],16)<<4)|parseInt(hex[i*2+1],16));
    };
    return new Uint8Array(arr);
};
function hex_join(arr) {
    let res = "";
    for (var i = 0; i < arr.length; ++i) {
	res += arr[i];
    }
    return res;
};

function hexs_to_bytes(arr) {
  return hex_to_bytes(hex_join(arr));
};

function u8_to_hex(num) {
  return ("00" + num.toString(16)).slice(-2);
};

function hex_to_u64(hex) {
  return parseInt(hex.slice(-64), 16);
};

function uN_to_hex(N, num) {
  var hex = "";
  for (var i = 0; i < N/4; ++i) {
    hex += hex_char[(num / (2**((N/4-i-1)*4))) & 0xF];
  };
  return hex;
};

function u32_to_hex(num) {
  return uN_to_hex(32, num);
};

function u64_to_hex(num) {
  return uN_to_hex(64, num);
};

function string_to_bytes(str) {
  return utf8_encoder.encode(str);
};

function bytes_to_string(buf) {
  return utf8_decoder.decode(buf);
};

function string_to_hex(str) {
  return bytes_to_hex(string_to_bytes(str));
};

function hex_to_string(hex) {
  return bytes_to_string(hex_to_bytes(hex));
};

function bytes_to_hex(buf) {
    var hex = "";
    for (var i = 0; i < buf.length; ++i) {
	hex += hex_char[buf[i]>>>4] + hex_char[buf[i]&0xF];
    };
    return hex;
};

function get_time() {
    return Date.now() + delta_time;  
};


module.exports = function client(url="ws://localhost:8080") {
    var ws = new WebSocket(url);
    var Posts = {};
    var watching = {};

    var on_init_callback = null;
    var on_post_callback = null;

    function ws_send(buffer) {
	if (ws.readyState === 1) {
	    ws.send(buffer);
	} else {
	    setTimeout(() => ws_send(buffer), 20);
	}
    }

    function on_post(callback) {
	on_post_callback = callback;
    }
    
    function on_init(callback) {
	on_init_callback = callback;
    }
    
    function get_post(room_id, from, to) {
	let msg_buff = hexs_to_bytes([
	    u8_to_hex(GET),
	    u64_to_hex(room_id),
	    u64_to_hex(from),
	    u64_to_hex(to)
	])
	ws_send(msg_buff);
    }

    function send_post(room_id, data) {
	if (string_to_hex(data).len > 1024*4) {
	    console.log("Cannot send more than 1024 bytes of data")
	}
	else {
	    let msg_buff = hexs_to_bytes([
		u8_to_hex(POST),
		u64_to_hex(room_id),
		string_to_hex(data)
	    ]);
	    ws_send(msg_buff);
	}
    }
    function watch_room(room_id) {
	if (!watching[room_id]) {
	    watching[room_id] = true;
	    let msg_buff = hexs_to_bytes([
		u8_to_hex(WATCH),
		u64_to_hex(room_id),
	    ]);
	    Posts[room_id] = [];
	    ws_send(msg_buff); 
	}
    };

    function unwatch_room(room_id) {
	if (watching[room_id]) {
	    watching[room_id] = false;
	    let msg_buff = hexs_to_bytes([
		u8_to_hex(UNWATCH),
		u64_to_hex(room_id),
	    ]);
	    ws_send(msg_buff); 
	}
    };

    function get_server_time() {
	let msg_buff = hexs_to_bytes([
	    u8_to_hex(TIME)
	]);
	ws_send(msg_buff);
    }

    ws.binaryType = "arraybuffer";

    ws.onopen = function () {
	if (on_init_callback) {
	    on_init_callback()
	}
    }

    ws.onmessage = (msg) => {
	var msg = new Uint8Array(msg.data);

	if (msg[0] == POST) {
	    var room = bytes_to_hex(msg.slice(1, 9));
	    var time = bytes_to_hex(msg.slice(9, 17));
	    var data = bytes_to_hex(msg.slice(17, msg.length));
	    
	    if (on_post_callback) {
		on_post_callback({room, time, data}, Posts);
	    }
	    console.log(room, time, data)
	}
	else if (msg[0] == TIME) {
	    var time = bytes_to_hex(msg.slice(1, 9));
	    console.log("Current server time: ", Date(time));
	}
    }
    return {
	send_post,
	get_post,
	watch_room,
	unwatch_room,
	get_server_time
    }
}

// let {send_post, get_post, watch_room} = client();
// send_post(0, "pinto");

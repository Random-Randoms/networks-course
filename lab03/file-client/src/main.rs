use std::{
    env,
    io::{Read, Write},
    net::TcpStream,
    string::String,
};

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let addr = format!("{}:{}", args[1], args[2]);
    let fname = &args[3];
    let mut sck = TcpStream::connect(&addr).unwrap();
    println!("connected to server at {}", addr);
    println!("requesting file {}", fname);

    let req = format!("GET /{} HTTP/1.1\r\n\r\n", fname);
    sck.write_all(req.as_bytes()).unwrap();
    sck.flush().unwrap();
    println!("request sent");


    let mut buf = [0u8; 1024];
    let mut bytes = Vec::<u8>::new();
    loop {
        println!("reading socket");
        let br = sck.read(&mut buf).unwrap();
        if br == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..br]);
    }
    println!("socket read");
    let mut start = 0usize;
    let mut found_code = false;
    while start < bytes.len() {
        if bytes[start..start + 1] == *b" " {
            if !found_code {
                if bytes[start + 1..start + 4] == *b"404" {
                    println!("no such file!");
                    return;
                }
            }
            found_code = true;
        }
        if bytes[start..].starts_with(b"\r\n\r\n") {
            let body = &bytes[start..];
            println!("file content:\n{}", String::from_utf8_lossy(body));
            return;
        }
        start += 1;
    }
    println!("bad response\n");
}

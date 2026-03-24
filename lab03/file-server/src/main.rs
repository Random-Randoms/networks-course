use std::{
    env,
    fs,
    io::prelude::*,
    net::TcpListener,
    thread,
    time::Duration,
    string::String,
    sync::{Arc, Condvar, Mutex},
};

fn parse_req(req: &[u8]) -> Option<String> {
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut reqp = httparse::Request::new(&mut headers);
    loop {
        let res = reqp.parse(req).ok()?;
        
        if let Some(ref path) = reqp.path {
            return Some(path[1..].to_owned());
        }
        
        if res.is_partial() {
            continue;
        } else {
            return None;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    let addr = format!("127.0.0.1:{}", args[1]);
    let mxthreads = str::parse::<usize>(args[2].as_str()).unwrap();
    let port_listener = TcpListener::bind(addr).unwrap();

    let threads = Arc::new(Mutex::new(1usize));
    let cvar = Arc::new(Condvar::new());

    for stream in port_listener.incoming() {
        match stream {
            Ok(s) => {
                let mut src = s.try_clone().unwrap();
                let mut snk = s.try_clone().unwrap();

                {
                    let threads_cln = threads.clone();
                    let mut grd = threads_cln.lock().unwrap();
                    grd = cvar.wait_while(grd, |thr| *thr >= mxthreads).unwrap();
                    *grd += 1;
                }

                let cvar_cln = cvar.clone();
                let threads_cln = threads.clone();
                thread::spawn(move || {
                    let mut buf = [0u8; 1024];
                    let path: String;
                    loop {
                        src.read(&mut buf);
                        match parse_req(&buf) {
                            Some(p) => {
                                path = p;
                                break;
                            },
                            _ => continue,
                        };
                    };
                    //std::thread::sleep(Duration::from_secs(2));
                    println!("path:\n {}", path);
                    match fs::read(path.as_str()) {
                        Ok(bytes) => {
                            let resp = format!(
                                "HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n",
                                bytes.len()
                            );
                            snk.write(resp.as_bytes());
                            snk.write(bytes.as_slice());
                            snk.flush();
                        },
                        _ => {
                            let resp = format!("HTTP/1.0 404 NOT FOUND\r\n");
                            snk.write(resp.as_bytes());
                            snk.flush();
                        }
                    }
                    
                    {
                        let mut grd = threads_cln.lock().unwrap();
                        *grd -= 1;
                        cvar_cln.notify_one();
                    }
                });
            }
            Err(_) => continue,
        }
    }

    Ok(())
}
use std::{net::UdpSocket, time::Duration};

use anyhow::anyhow;

fn main() -> anyhow::Result<()> {
    let sock = UdpSocket::bind("127.0.0.1:8878")?;
    sock.set_read_timeout(Some(Duration::from_secs(1)))?;

    let mut buf = [0u8; 1024];

    loop {
        let (size, _) = match sock.recv_from(&mut buf) {
            Ok(x) => x,
            Err(_) => break,
        };
        let recv_time = chrono::Utc::now();
        let req =  String::from_utf8_lossy(&buf[..size]);
    
        print!("received message {}", req);
        if rand::random_bool(0.8) {
            let send_time = chrono::DateTime::parse_from_rfc3339(req
                .split_ascii_whitespace()
                .skip(2)
                .next().ok_or(anyhow!("malformed heartbeat message"))?
            )?;
            let elapsed = recv_time.timestamp_micros() - send_time.timestamp_micros();
            println!(", flight time = {elapsed}μs");
        } else {
            println!(", it was lost...");
        }
    }

    Err(anyhow!("Heartbeat timed out"))
}

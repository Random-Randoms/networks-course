use std::{
    net::{ToSocketAddrs, UdpSocket},
    time::Duration,
};

use clap::Parser;

fn message(seq: i32) -> String {
    format!("Heartbeat {seq} {}", chrono::Utc::now().to_rfc3339())
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    server: String,

    #[arg(long)]
    client: String,
}

fn main() -> anyhow::Result<()>{
    let args = Args::parse();
    let addr = args.server.to_socket_addrs()?.into_iter().next().unwrap();
    let sock = UdpSocket::bind(args.client)?;

    let mut seq = 0;
    loop {
        sock.send_to(message(seq).as_bytes(), addr)?;
        seq += 1;
        std::thread::sleep(Duration::from_millis(500));
    }
}

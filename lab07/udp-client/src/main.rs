use std::{
    net::{ToSocketAddrs, UdpSocket},
    time::Duration,
};

use anyhow::anyhow;
use clap::Parser;

const PINGS: i8 = 10;

fn message(seq: i8) -> String {
    format!("Ping {seq} {}", chrono::Local::now().to_rfc3339())
}

fn stats(rtts: Vec<i64>) -> Option<(i64, i64, i64, i16)> {
    if rtts.is_empty() {
        return None;
    }
    let avg = rtts.iter().sum::<i64>() as f64 / rtts.len() as f64;
    let min = rtts.iter().min().unwrap();
    let max = rtts.iter().max().unwrap();
    let lost = (PINGS - rtts.len() as i8) as f32 / PINGS as f32 * 100f32;
    Some((*min, *max, avg.floor() as i64, lost.round() as i16))
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    addr: String,
}

fn main() -> anyhow::Result<()>{
    let args = Args::parse();
    let addr = "127.0.0.1:8878".to_socket_addrs()?.into_iter().next().unwrap();
    let sock = UdpSocket::bind(args.addr)?;

    (0..PINGS).map(|seq| {
        sock.send_to(message(seq).as_bytes(), addr)
    }).collect::<Result<Vec<_>, _>>()?;

    let mut rtts = Vec::<i64>::new();
    let mut buf = [0u8; 1024];
    sock.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut left = PINGS;
    while left > 0 {
        let size = match sock.recv_from(&mut buf) {
            Ok((x, sender)) => {
                if sender == addr {
                    x
                } else {
                    continue;
                }
            },
            Err(_) => {
                // timeout
                break;
            }
        };
        let recv_time = chrono::Local::now();
        let rsp = String::from_utf8_lossy(&buf[..size]);
        print!("{rsp}");
        let send_time = chrono::DateTime::parse_from_rfc3339(rsp
            .split_ascii_whitespace()
            .skip(2)
            .next()
            .ok_or(anyhow!("malformed message"))?)?;
        let rtt = recv_time.timestamp_micros() - send_time.timestamp_micros();
        println!(" rtt={rtt}μs");
        rtts.push(rtt);
        left -= 1
    };

    match stats(rtts) {
        None => println!("no packets received"),
        Some((min, max, avg, lost)) => 
            println!("min: {min}μs, max: {max}μs, avg: {avg}μs, lost: {lost}%")
    }

    Ok(())
}

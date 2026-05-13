use std::{
    fs::File,
    io::{BufReader, Read},
    net::{ToSocketAddrs, UdpSocket},
    time::Duration,
};

use clap::Parser;

const HEADER_SIZE: u8 = 4;
const LOSS_CHANCE: f64 = 0.3;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    server_addr: String,

    #[arg(long)]
    client_addr: String,

    #[arg(long)]
    timeout: u64,

    #[arg(long)]
    file: String,
}

fn checksum(buf: &[u8]) -> [u8; 2] {
    let mut res = 0u16;
    for i in 0..buf.len() / 2 {
        res = res.wrapping_add(buf[2 * i] as u16 + ((buf[2 * i + 1] as u16) << 8));
    }
    if buf.len() % 2 == 1 {
        res = res.wrapping_add(buf[buf.len() - 1] as u16);
    }
    res = 0xffff - res;
    [(res & 0xff) as u8, (res >> 8) as u8]
}

fn main() -> anyhow::Result<()>{
    let args = Args::parse();
    let server_addr = args.server_addr.to_socket_addrs()?.into_iter().next().unwrap();
    let sock = UdpSocket::bind(args.client_addr)?;
    sock.set_read_timeout(Some(Duration::from_secs(args.timeout)))?;
    let mut reader = BufReader::new(File::open(args.file)?);
    let mut send = [0u8; 128];
    let mut recv = [0u8; 1];
    let mut ind = 0u8;
    let mut sent = 0usize;
    let mut finish = false;
    loop { // iterate over file to send
        let read = reader.read(&mut send[HEADER_SIZE as usize..])?;
        if read == 0 {
            println!("IND: {ind} | SENT: {sent:6} | sending final empty package");
            finish = true;
        }
        send[0] = read.try_into().unwrap();
        send[1] = ind;
        send[2] = 0;
        send[3] = 0;
        let chsum = checksum(&send[..read + HEADER_SIZE as usize]);
        send[2] = chsum[0];
        send[3] = chsum[1];
        'send: loop { // try to send piece of file
            println!("IND: {ind} | SENT: {sent:6} | send package: index {ind}, {read} bytes");
            if rand::random_bool(1.0 - LOSS_CHANCE) {
                sock.send_to(&send[..read + HEADER_SIZE as usize], server_addr)?;
            } else {
                println!("IND: {ind} | SENT: {sent:6} | package we are trying to send was lost");
            }
            loop { // try to receive ack
                match sock.recv_from(&mut recv) {
                    Ok((_, addr)) => {
                        if addr != server_addr {
                            continue;
                        }
                    },
                    Err(_) => {
                        println!("IND: {ind} | SENT: {sent:6} | recv timed out");
                        continue 'send;
                    }
                }
                break;
            }
            if ind != recv[0] {
                println!("IND: {ind} | SENT: {sent:6} | received incorrect ACK index");
                continue;
            }
            ind = (ind + 1) & 1;
            sent += read;
            break;
        }
        if finish {
            break;
        }
    }
    println!("finish sending");
    Ok(())
}

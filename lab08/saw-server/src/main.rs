use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    net::{ToSocketAddrs, UdpSocket},
    thread::sleep,
    time::Duration,
};

use anyhow::anyhow;
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

fn check(buf: &[u8]) -> anyhow::Result<()> {
    let mut res = 0u16;
    for i in 0..buf.len() / 2 {
        res = res.wrapping_add(buf[2 * i] as u16 + ((buf[2 * i + 1] as u16) << 8));
    }
    if buf.len() % 2 == 1 {
        res = res.wrapping_add(buf[buf.len() - 1] as u16);
    }
    if res != 0xffff {
        return Err(anyhow!("incorrect checksum"));
    }
    Ok(())
}

fn main() -> anyhow::Result<()>{
    let args = Args::parse();
    let sock = UdpSocket::bind(args.server_addr)?;
    let client_addr = args.client_addr.to_socket_addrs()?.into_iter().next().unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(args.timeout)))?;
    let mut writer = BufWriter::new(OpenOptions::new()
        .create_new(true)
        .write(true)
        .append(true)
        .open(args.file)?
    );
    let mut recv = [0u8; 128];
    let mut recvd = 0usize;
    let mut send = [0u8; 1];
    let mut ind = 0u8;
    'recv: loop { // recv new package
        sleep(Duration::from_millis(100));
        let mut recv_bytes = 0u8;
        loop { // get new piece of package
            match sock.recv_from(&mut recv[recv_bytes as usize..]) {
                Ok((x, addr)) => {
                    if addr != client_addr {
                        continue;
                    }
                    recv_bytes += x as u8;
                    if recv[0] + HEADER_SIZE  == recv_bytes {
                        break;
                    } else {
                        continue;
                    }
                },
                Err(_) => {
                    println!("IND: {ind} | RECV {recvd:4} | recv timed out");
                    continue 'recv;
                }
            }
        }
        println!("IND: {ind} | RECV {recvd:4} | received package, sending ack");
        if rand::random_bool(1.0 - LOSS_CHANCE) {
            send[0] = recv[1];
            sock.send_to(&send, client_addr)?;
        } else {
            println!("IND: {ind} | RECV {recvd:4} | package we are trying to send was lost");
        }
        if ind == recv[1] {
            println!("IND: {ind} | RECV {recvd:4} | received data");
            if let Err(_) = check(&recv[..recv_bytes as usize]) {
                println!("checksum incorrect, drop package");
                continue;
            }
            ind = (ind + 1) & 1;
            recvd += (recv_bytes - HEADER_SIZE) as usize;
            
            writer.write_all(&mut recv[HEADER_SIZE as usize..recv_bytes as usize])?;
        } else {
            println!("IND: {ind} | RECV {recvd:4} | received incorect index");
        }
        if recv_bytes == HEADER_SIZE {
            println!("IND: {ind} | RECV {recvd:4} | received empty package");
            break;
        }
    }
    println!("finish receiving");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check, checksum};

    #[test]
    fn checksum_correct_odd() {
        let mut data = *b"fo  obarbaz";
        data[2] = 0;
        data[3] = 0;
        let sm = checksum(&data);
        data[2] = sm[0];
        data[3] = sm[1];
        assert!(matches!(check(&data), Result::Ok(_)));
    }

    #[test]
    fn checksum_correct_even() {
        let mut data = *b"ba  rabaraberebere";
        data[2] = 0;
        data[3] = 0;
        let sm = checksum(&data);
        data[2] = sm[0];
        data[3] = sm[1];
        assert!(matches!(check(&data), Result::Ok(_)));
    }

    #[test]
    fn checksum_wrong() {
        let mut data = *b"fo  obarbazz";
        data[2] = 0;
        data[3] = 0;
        let sm = checksum(&data[..10]);
        data[2] = sm[0];
        data[3] = sm[1];
        data[1] = data[1] - 1;
        assert!(matches!(check(&data), Result::Err(_)));
    }
}

use anyhow::anyhow;
use clap::Parser;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use dns_lookup::{lookup_addr, lookup_host};

use core::{net::SocketAddr};
use std::{io::Read, net::{IpAddr, Ipv4Addr}, time::{Duration, Instant}};

const MAGIC_NUMBER: u16 = 0x1234;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    target: String,

    #[arg(short, long, default_value_t = 3)]
    probes: u8,

    #[arg(short('T'), long, default_value_t = 1500)]
    timeout: u64,
}

struct Icmp {
    id: u16,
    seq: u16,
}

fn checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    data.iter().enumerate().for_each(|(i, &b)| {
        sum += (b as u32) << (i % 2 * 8);
    });
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

impl Icmp {
    fn build(self) -> [u8; 12] {
        let mut data = [0u8; 12];
        data[0] = 8;
        data[4] = (self.id & 0xFF) as u8;
        data[5] = (self.id >> 8) as u8;
        data[6] = (self.seq & 0xFF) as u8;
        data[7] = (self.seq >> 8) as u8;
        let icmp_csum = checksum(&data);
        //data[10] = (ip_csum & 0xff) as u8;
        //data[11] = (ip_csum >> 8) as u8;
        data[2] = (icmp_csum & 0xff) as u8;
        data[3] = (icmp_csum >> 8) as u8;
        data
    }
}

struct IcmpReply {
    id: u16,
    seq: u16,
    ty: u8,
}

impl IcmpReply {
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }
        //if bytes[0] != 0 || bytes[1] != 0 {
        //    return None;
        //}
        Some(IcmpReply { 
            ty: bytes[0],
            id: bytes[4] as u16 + ((bytes[5] as u16) << 8),
            seq: bytes[6] as u16 + ((bytes[7] as u16) << 8),
        })
    }
}

fn ip_to_name(addr: IpAddr) -> String {
    match lookup_addr(&addr) {
        Ok(name) => name,
        Err(_) => format!("{addr}"),
    }
}

fn name_to_ip<'a>(name: &'a str) -> Option<IpAddr> {
    lookup_host(name)
        .ok()
        .and_then(|mut i| i.next())
}

fn main() -> anyhow::Result<()>{
    let args = Args::parse();
    let source_addr = Ipv4Addr::UNSPECIFIED;
    let source_sock_addr = SockAddr::from(SocketAddr::new(IpAddr::V4(source_addr), 0));
    let target_addr = name_to_ip(args.target.as_str()).ok_or(anyhow!("couldn't resolve name"))?;
    let target_sock_addr = SockAddr::from(SocketAddr::new(target_addr, 8080));
    let probes = args.probes;
    let mut socket = Socket::new_raw(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
    println!("made socket");
    socket.bind(&source_sock_addr)?;
    println!("bound socket");
    socket.set_read_timeout(Some(Duration::from_millis(args.timeout)))?;
    println!("timeout set");

    let mut ttl = 1;
    loop {
        socket.set_ttl_v4(ttl as u32)?;
        let send_time = (0..probes).map(|probe| {
            let pkt = Icmp {
                id: MAGIC_NUMBER,
                seq: probe as u16 + ((ttl as u16) << 8),
            };
            //socket.send(&pkt.build()).map(|_| Instant::now())
            socket.send_to(&pkt.build(), &target_sock_addr).map(|_| Instant::now())
        }).collect::<Result<Vec<_>, _>>()?;
        //println!("sent echos");

        let mut recv_left = probes;
        let mut sender: Option<IpAddr> = None;
        let mut recv_time = Vec::<Option<Instant>>::new();
        recv_time.resize(probes as usize, None);
        let mut buf = [0u8; 1024];
        while recv_left > 0 {
            let bytes = match socket.read(&mut buf) {
                Ok(x) => x,
                Err(_) => break,
            };
            let time = Instant::now();
            if bytes < 28 {
                println!("not enought bytes");
                continue;
            }
            //println!("ihl: {}", buf[0] & 0xf);
            sender = Some(IpAddr::V4(Ipv4Addr::from_octets(*buf[12..16].as_array().unwrap())));
            let ip_header_len = (buf[0] & 0xf) as usize * 4;
            let icmp_type = IcmpReply::from_bytes(&buf[ip_header_len..]).unwrap().ty;
            let pad = match icmp_type {
                0 => ip_header_len,
                11 => ip_header_len + 8 + 20,
                _ => {println!("wrong icmp type"); continue},
            };
            let Some(reply) = IcmpReply::from_bytes(&buf[pad..pad + 8]) else {
                println!("icmp parse failed");
                continue;
            };
            if reply.id != MAGIC_NUMBER {
                println!("wrong magic number");
                continue;
            }
            if (reply.seq >> 8) as u8 != ttl {
                println!("wrong seq");
                continue;
            }
            recv_left -= 1;
            let probe = (reply.seq & 0xff) as usize;
            *recv_time.get_mut(probe).ok_or(anyhow!("seq value too big"))? = Some(time);
        }
        print!("{:3} | ", ttl);
        send_time.into_iter().zip(recv_time).for_each(|(s, r)| {
            match r {
                None => print!("    *    "),
                Some(r) => {
                    let elapsed = r.duration_since(s).as_millis();
                    print!(" {:5}ms ", elapsed);
                }
            }
        });
        match sender.clone() {
            None => println!(" | ? "),
            Some(s) => println!(" | {}", ip_to_name(s)),
        }
        if let Some(s) = sender {
            if s == target_addr {
                break;
            }
        };
        ttl += 1;
    }


    Ok(())
}

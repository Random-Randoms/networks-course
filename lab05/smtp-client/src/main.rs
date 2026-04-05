use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream
};

use anyhow::anyhow;
use clap::Parser;
use base64::{Engine, engine::general_purpose::STANDARD};

trait SmptSocket {
    fn read_code(&mut self) -> Result<i16, anyhow::Error>;

    fn send(&mut self, data: &str) -> Result<(), anyhow::Error>;

    fn quit(&mut self) -> Result<(), anyhow::Error> {
        self.send(format!("QUIT\r\n").as_str())
    }

    fn send_expext_code(&mut self, data: &str, code: i16) -> Result<(), anyhow::Error> {
        self.send(data)?;
        if code != self.read_code()? {
            self.quit()?;
            Err(anyhow!("code {code} not received"))
        } else {
            Ok(())
        }
    }
}


impl SmptSocket for TcpStream {
    fn read_code(&mut self) -> Result<i16, anyhow::Error> {
        let mut buf = [0u8; 1024];
        let mut bytes_read: usize;
        let mut bytes = Vec::<u8>::new();
        loop {
            bytes_read = self.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..bytes_read]);
            println!("server answer: {}", String::from_utf8_lossy(&buf[..bytes_read]));
            if bytes.ends_with(b"\r\n") {
                break;
            }
        }
        
        if bytes.len() < 3 {
            return Err(anyhow!("answer does not contain code"));
        }
        return Ok(str::from_utf8(&bytes[..3])?.parse()?);
    }

    fn send(&mut self, data: &str) -> Result<(), anyhow::Error> {
        self.write(data.as_bytes())?;
        Ok(())
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    sender_addr: String,

    #[arg(long, default_value=None)]
    sender_name: Option<String>,

    #[arg(long)]
    receiver_addr: String,

    #[arg(long, default_value=None)]
    receiver_name: Option<String>,

    #[arg(short, long, default_value="")]
    subject: String,

    #[arg(long)]
    message: String,

    #[arg(long)]
    picture: Option<String>,

    #[arg(short, long)]
    creds: String,
}

fn main() -> Result<(), anyhow::Error> {
    println!("read creds");
    let args = Args::parse();
    let creds_text = std::fs::read_to_string(args.creds)?;
    let (smtp_uname, smtp_pword) = creds_text
        .split_once(" ")
        .expect("creds file should contain only smpt username and password, separated by space");
    println!("connect to server\n");
    let mut stream = TcpStream::connect("localhost:2525")?;
    //stream.set_nonblocking(true)?;
    stream.read_code()?;

    println!("send HELO");
    stream.send_expext_code(format!("HELO {smtp_uname}\r\n").as_str(), 250)?;
    println!("send AUTH LOGIN");
    stream.send_expext_code(format!("AUTH LOGIN \r\n").as_str(), 334)?;
    println!("send username");
    stream.send_expext_code(format!("{}\r\n", STANDARD.encode(smtp_uname)).as_str(), 334)?;
    println!("send password");
    stream.send_expext_code(format!("{}\r\n", STANDARD.encode(smtp_pword)).as_str(), 235)?;
    println!("send MAIL FROM");
    stream.send_expext_code(format!("MAIL FROM: <{smtp_uname}>\r\n").as_str(), 250)?;
    println!("send RCPT TO");
    stream.send_expext_code(format!("RCPT TO: <{}>\r\n", args.receiver_addr).as_str(), 250)?;
    println!("senf DATA");
    stream.send_expext_code(format!("DATA\r\n").as_str(), 354)?;
    let mut msg = String::new();
    if let Some(name) = args.sender_name {
        msg += format!("From: {} <{}>\r\n", name, args.sender_addr).as_str();
    }
    if let Some(name) = args.receiver_name {
        msg += format!("To: {} <{}>\r\n", name, args.receiver_addr).as_str();
    }
    msg += format!("Subject: {}\r\n", args.subject).as_str();
    msg += "MIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=frontier\r\n\r\n";
    msg += "--frontier\r\nContent-Type: text/plain\r\n\r\n";
    BufReader::new(File::open(args.message)?)
        .lines()
        .try_for_each(|l| {
            l.map(|line| msg += format!("{}\r\n", line).as_str())
        })?;
    if let Some(picture) = args.picture {
        msg += "\r\n--frontier\r\nContent-Type: image/png\r\nContent-Transfer-Encoding: base64\r\n\r\n";
        let bytes = BufReader::new(File::open(picture)?).bytes();
        let mut v = Vec::<u8>::new();
        bytes.for_each(|b| b.iter().for_each(|byte| v.push(*byte)));
        let data = STANDARD.encode(v);
        msg += data.as_str();
        msg += "\r\n";
    }
    msg += "--frontier--\r\n";

    msg += ".\r\n";
    println!("send message");
    stream.send_expext_code(msg.as_str(), 250)?;
    println!("quit");
    stream.quit()?;

    Ok(())
}

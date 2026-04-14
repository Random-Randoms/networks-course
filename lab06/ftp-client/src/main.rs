use std::{
    io::{Read, Write},
    net::TcpStream,
    ops::Range,
};

use anyhow::anyhow;
use clap::Parser;

trait FtpSocket {
    fn read_code(&mut self) -> anyhow::Result<i16>;

    fn send(&mut self, data: &str) -> anyhow::Result<()>;

    fn quit(&mut self) -> anyhow::Result<()> {
        self.send(format!("QUIT\r\n").as_str())
    }

    fn expect_code(&mut self, range: Range<i16>) -> anyhow::Result<()> {
        let actual = self.read_code()?;
        if !range.contains(&actual) {
            anyhow::bail!("Incorrect code {actual}");
        } else {
            Ok(())
        }

    }

    fn read_response(&mut self) -> anyhow::Result<String>;

    fn send_expect_code(&mut self, data: &str, range: Range<i16>) -> anyhow::Result<()> {
        self.send(data)?;
        if !range.contains(&self.read_code()?) {
            self.quit()?;
            Err(anyhow!("Incorrect code"))
        } else {
            Ok(())
        }
    }

    fn send_read_response(&mut self, data: &str) -> anyhow::Result<String> {
        self.send(data)?;
        self.read_response()
    }

    fn read_all(&mut self) -> anyhow::Result<String>;

    fn read_all_bytes(&mut self) -> anyhow::Result<Vec<u8>>;
}


impl FtpSocket for TcpStream {
    fn read_code(&mut self) -> anyhow::Result<i16> {
        let mut buf = [0u8; 1024];
        let mut bytes_read: usize;
        let mut bytes = Vec::<u8>::new();
        loop {
            bytes_read = self.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..bytes_read]);
            //println!("server answer: {}", String::from_utf8_lossy(&buf[..bytes_read]));
            if bytes.ends_with(b"\r\n") {
                break;
            }
        }
        
        if bytes.len() < 3 {
            return Err(anyhow!("answer does not contain code"));
        }
        return Ok(str::from_utf8(&bytes[..3])?.parse()?);
    }

    fn read_response(&mut self) -> anyhow::Result<String> {
        let mut buf = [0u8; 1024];
        let mut bytes_read: usize;
        let mut bytes = Vec::<u8>::new();
        loop {
            bytes_read = self.read(&mut buf)?;
            if bytes_read == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..bytes_read]);
            //println!("server answer: {}", String::from_utf8_lossy(&buf[..bytes_read]));
            if bytes.ends_with(b"\r\n") {
                break;
            }
        }
        
        if bytes.len() < 3 {
            return Err(anyhow!("answer does not contain code"));
        }
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }

    fn send(&mut self, data: &str) -> anyhow::Result<()> {
        self.write(data.as_bytes())?;
        Ok(())
    }

    fn read_all(&mut self) -> anyhow::Result<String> {
        let mut bytes = Vec::<u8>::new();
        self.read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(bytes.as_slice()).to_string())
    }

    fn read_all_bytes(&mut self) -> anyhow::Result<Vec<u8>> {
        let mut bytes = Vec::<u8>::new();
        self.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

struct FtpConnection {
    sock: TcpStream,
}

impl FtpConnection {
    fn open(addr: String) -> anyhow::Result<Self> {
        let mut sock = TcpStream::connect(addr)?; 
        sock.read_code()?;
        Ok(Self { sock })
    }

    fn send_creds<'a>(mut self, uname: &'a str, pword: &'a str) -> anyhow::Result<Self> {
        self.sock.send_expect_code(format!("USER {uname}\r\n").as_str(), 300..400)?;
        self.sock.send_expect_code(format!("PASS {pword}\r\n").as_str(), 200..300)?;
        Ok(self)
    }

    fn type_i(mut self) -> anyhow::Result<Self> {
        self.sock.send_expect_code("TYPE I\r\n",   200..300)?;
        Ok(self)
    }

    fn go_passive(&mut self) -> anyhow::Result<TcpStream> {
        let resp = self.sock.send_read_response("PASV\r\n")?;
        let code: i16 = resp[..3].parse()?;
        if code != 227 {
            return Err(anyhow!("PASV returned not 227"));
        }
        let malf_err = "PASV response malformed";
        let mut addr_bytes = resp
            .split_once("(")
            .ok_or_else(|| anyhow!(malf_err))?
            .1
            .split_once(")")
            .ok_or_else(|| anyhow!(malf_err))?
            .0
            .split_terminator(",")
            .map(|s| s.parse::<u16>());
        let addr = format!(
            "{}.{}.{}.{}:{}",
            addr_bytes.next().ok_or_else(|| anyhow!(malf_err))??,
            addr_bytes.next().ok_or_else(|| anyhow!(malf_err))??,
            addr_bytes.next().ok_or_else(|| anyhow!(malf_err))??,
            addr_bytes.next().ok_or_else(|| anyhow!(malf_err))??,
            addr_bytes.next().ok_or_else(|| anyhow!(malf_err))?? * 256 
                + addr_bytes.next().ok_or_else(|| anyhow!(malf_err))??,
        );
        Ok(TcpStream::connect(addr)?)
    }

    fn ls(&mut self) -> anyhow::Result<Vec<RemoteFile>> {
        let data  = {
            let mut dat = self.go_passive()?;
            self.sock.send_expect_code("MLSD\r\n", 100..200)?;
            dat.read_all()?
        };
        self.sock.expect_code(200..300)?;
        Ok(data
            .split_terminator("\n")
            .filter_map(|f| {
                let (facts, fname) = f
                    .split_once(" ")?;
                let ftype = facts.split_terminator(";")
                    .filter_map(|fct| fct.split_once("="))
                    .filter(|(key, _)| key.to_ascii_lowercase().as_str() == "type")
                    .next()?.1;
                match ftype {
                    "dir" => Some(RemoteFile::Dir(fname.trim().to_string())),
                    "file" => Some(RemoteFile::File(fname.trim().to_string())),
                    _ => None,
                }
            }).collect::<Vec<_>>())
    }

    fn cd<'a>(&mut self, dir: &'a str) -> anyhow::Result<()> {
        self.sock.send_expect_code(format!("CWD {dir}\r\n").as_str(), 200..300)
    }

    fn load<'a>(&mut self, remote_name: &'a str) -> anyhow::Result<Vec<u8>> {
        let data = {
            let mut dat = self.go_passive()?;
            self.sock.send_expect_code(format!("RETR {remote_name}\r\n").as_str(), 100..200)?;
            dat.read_all_bytes()?
        };
        self.sock.expect_code(200..300)?;
        
        Ok(data)
    }

    fn store<'a>(&mut self, data: Vec<u8>, name: &'a str) -> anyhow::Result<()> {
        {
            let mut dat = self.go_passive()?;
            self.sock.send_expect_code(format!("STOR {name}\r\n").as_str(), 100..200)?;
            dat.write_all(data.as_slice())?;
        }
        self.sock.expect_code(200..300)
    } 

    fn quit(&mut self) -> anyhow::Result<()> {
        self.sock.send("QUIT\r\n")?;
        Ok(())
    }
}

#[derive(Debug)]
enum RemoteFile {
    Dir(String),
    File(String),
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    addr: String,

    #[arg(short, long)]
    creds: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let creds_text = std::fs::read_to_string(args.creds)?;
    let (ftp_uname, ftp_pword) = creds_text
        .split_once(" ")
        .expect("creds file should contain only smpt username and password, separated by space");
    let mut ftp = FtpConnection::open(args.addr)?
        .send_creds(ftp_uname, ftp_pword)?
        .type_i()?;

    let stdin = std::io::stdin();

    loop {
        let mut cmd = "".to_string();
        stdin.read_line(&mut cmd)?;
        let args = cmd.trim().split_terminator(" ").collect::<Vec<_>>();
        match args[0] {
            "ls" => {
                ftp
                    .ls()?
                    .iter()
                    .for_each(|f| match f {
                        RemoteFile::Dir(s) => println!("dir  : {s}"),
                        RemoteFile::File(s) => println!("file : {s}"),
                    });
                println!("");
            },
            "load" => {
                let file = args[1];
                let targ = args[2];
                std::fs::write(targ, ftp.load(&file)?)?;
                println!("file {file} loaded succesfully\n");
            },
            "store" => {
                let file = args[1];
                let targ = args[2];
                let data = std::fs::read(file)?;
                ftp.store(data, &targ)?;
                println!("file {file} stored succesfully\n");
            },
            "cd" => {
                let dir = args[1];
                ftp.cd(dir)?;
                println!("dir changed to {dir}\n");
            }
            _ => break,
        }
    }

    ftp.quit()?;

    Ok(())
}

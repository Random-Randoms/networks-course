use std::{
    env, io::{BufReader, BufWriter, Read, Write}, net::{TcpListener, TcpStream}, os::unix::ffi::OsStrExt, process::{Command, Stdio}
};

use anyhow::anyhow;

trait RemoteControlSocket {
    fn read_commands(&mut self) -> Result<Vec<String>, anyhow::Error>;
}

impl RemoteControlSocket for TcpStream {
    fn read_commands(&mut self) -> Result<Vec<String>, anyhow::Error> {
        let mut buf = [0u8; 1024];
        let mut bytes_read: usize;
        let mut bytes = Vec::<u8>::new();
        loop {
            bytes_read = self.read(&mut buf)?;
            bytes.extend_from_slice(&buf[..bytes_read]);
            if bytes.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        
        let mut result=  String::from_utf8_lossy(&bytes[..bytes.len() - 2])
            .to_string()
            .split("\r\n")
            .map(|s| s.to_owned())
            .collect::<Vec<_>>();
        result.pop();

        Ok(result)
    }
}    

fn main() -> Result<(), anyhow::Error> {
    let mut args = env::args().skip(1);
    let addr = format!(
        "{}:{}",
        args.next().expect("first argument should contain address"),
        args.next().expect("second argument should contain port"),
    );
    let listener = TcpListener::bind(addr)?;

    //println!("{}", String::from_utf8_lossy(Command::new("lscpu").output()?.stdout.as_slice()));

    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                let mut sink = BufWriter::new(stream.try_clone()?);
                let cmds = stream.read_commands()?;
                let mut command = Command::new(cmds[0].clone());
                cmds.iter().skip(1).for_each(|arg| { command.arg(arg); });
                print!("received command:\n{}", String::from_utf8_lossy(command.get_program().as_bytes()));
                command.get_args().for_each(|arg| print!(" {}", String::from_utf8_lossy(arg.as_bytes())));
                println!("");
                let mut output = BufReader::new(
                    command.stdout(Stdio::piped())
                    .spawn()?
                    .stdout
                    .take()
                    .ok_or(anyhow!("process has no stdout"))?
                );
                let mut buf = [0u8; 32];
                while let Ok(read) = output.read(&mut buf) {
                    sink.write_all(&buf[..read])?;
                    sink.flush()?;
                    if read == 0 {
                        break;
                    }
                };
                sink.write(b" ------------------------ \r\n")?;
                sink.write(b" -- program terminated -- \r\n")?;
                sink.write(b" ------------------------ \r\n")?;
                sink.flush()?;
                println!("sent results to client");
            },
            Err(_) => continue,
        }
    }

    Ok(())
}

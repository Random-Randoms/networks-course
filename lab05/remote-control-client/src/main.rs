use std::{
    env,
    io::{BufWriter, Read, Write},
    net::TcpStream,
};

fn main() -> Result<(), anyhow::Error> {
    let mut stream = TcpStream::connect("127.0.0.1:8080")?;
    let mut writer = BufWriter::new(stream.try_clone()?);
    env::args().skip(1).try_for_each(|arg| {
        writer.write_all(arg.as_bytes()).and_then(|_| writer.write_all(b"\r\n"))
    })?;
    writer.write_all(b"\r\n")?;
    writer.flush()?;
    
    let mut buf = [0u8; 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    break;
                }
                print!("{}", String::from_utf8_lossy(&buf[..bytes_read]));
            },
            Err(_) => break,
        }
    }

    Ok(())
}

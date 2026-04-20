use std::net::UdpSocket;


fn main() -> anyhow::Result<()> {
    let sock = UdpSocket::bind("127.0.0.1:8878")?;

    let mut buf = [0u8; 1024];

    loop {
        let (size, addr) = sock.recv_from(&mut buf)?;
        let req =  String::from_utf8_lossy(&buf[..size]);
        println!("received message {}", req);
        let rsp = req.to_ascii_uppercase();
        if rand::random_bool(0.8) {
            println!("send {rsp} back");
            sock.send_to(rsp.as_bytes(), addr)?;
        } else {
            println!("message was lost...");
        }
    }
}

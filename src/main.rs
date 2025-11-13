use std::net::UdpSocket;

fn main() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:8080")?;
    let mut buf = [0 as u8; 1024];
    println!("监听 127.0.0.1:8080");
    loop {
        let (amt, src) = socket.recv_from(&mut buf)?;
        println!("收到 {} 字节数据 {:?} 来自 {:?}", amt, &buf[..amt], src);
        socket.send_to(&buf[..amt], src)?;
    }
}

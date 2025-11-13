use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:8080").await?;
    println!("异步UDP服务器启动");

    let mut buf = [0u8; 1024];

    loop {
        // 异步recv_from：非阻塞
        let (len, src_addr) = socket.recv_from(&mut buf).await?;
        let msg = String::from_utf8_lossy(&buf[..len]);
        println!("收到: {} from {}", msg, src_addr);

        // 异步send_to
        socket.send_to(&buf[..len], src_addr).await?;
    }
}
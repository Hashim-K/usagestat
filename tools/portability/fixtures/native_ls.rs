use std::{io::{Read, Write}, net::TcpListener};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |key: &str| args.windows(2).find(|pair| pair[0] == key).unwrap()[1].clone();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", get("--port"))).unwrap();
    let expected = format!("x-fixture-token: {}", get("--csrf_token"));
    for mut stream in listener.incoming().flatten() {
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        while let Ok(size) = stream.read(&mut buffer) {
            if size == 0 { break; }
            request.extend_from_slice(&buffer[..size]);
            if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") || request.len() > 16384 { break; }
        }
        let valid = String::from_utf8_lossy(&request).lines().any(|line| line == expected);
        let status = if valid { "200 OK" } else { "401 Unauthorized" };
        let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{{\"ok\":true}}");
    }
}

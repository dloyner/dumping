use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

use native_tls::{Certificate, TlsConnector};

fn main() {
    let root = Path::new(env!("WORKSPACE_ROOT"));
    let pem = std::fs::read(root.join("http-server/cert.pem")).unwrap();
    let cert = Certificate::from_pem(&pem).expect("cant make Cert with pem");

    let connector = TlsConnector::builder()
        .add_root_certificate(cert)
        .build()
        .unwrap();

    let stream = TcpStream::connect("127.0.0.1:8080").unwrap();
    let mut stream = connector.connect("127.0.0.1", stream).unwrap();

    let json_body = r#"{"message": "Hello, axum!"}"#;
    let request = format!(
        "POST /api/data HTTP/1.1\r\n\
        Host: 127.0.0.1\r\n\
        Content-Type: application/json\r\n\
        Content-Length: {}\r\n\
        Connection: close\r\n\
        \r\n\
        {}",
        json_body.len(),
        json_body
    );
    stream.write_all(request.as_bytes()).unwrap();

    let mut buffer = [0; 512];
    let mut response = String::new();

    while let Ok(bytes_read) = stream.read(&mut buffer) {
        if bytes_read == 0 {
            break;
        }
        response.push_str(&String::from_utf8_lossy(&buffer[..bytes_read]));
    }

    println!("Response: {response}")
}

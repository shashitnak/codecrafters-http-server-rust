use std::{net::{TcpListener, TcpStream}, thread, io::{self, Write, BufRead, BufReader}};

fn handle_client(mut stream: TcpStream) -> io::Result<()> {
    let mut _data = vec![];
    let mut reader = BufReader::new(&stream);
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.as_str() == "\r\n" {
            break;
        }
        _data.extend_from_slice(line.as_bytes());
    }
    stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n")?;
    Ok(())
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("accepted new connection");
                thread::spawn(|| handle_client(stream));
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

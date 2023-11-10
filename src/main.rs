use std::{net::{TcpListener, TcpStream}, thread, io::{self, Write, BufRead, BufReader}};

trait HttpStatus: Sized
where
    usize: TryFrom<Self>,
{
    fn as_msg(self) -> Option<&'static str> {
        let status_code: usize = self
            .try_into()
            .ok()?;

        Some(match status_code {
            200 => "200 OK",
            404 => "404 Not Found",
            _ => None?
        })
    }
}

impl<T> HttpStatus for T
where
    usize: TryFrom<T> {}

fn handle_client(mut stream: TcpStream) -> io::Result<()> {
    let mut _data = vec![];
    let mut reader = BufReader::new(&stream);
    let mut status_code = 404;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.as_str() == "\r\n" {
            break;
        }
        if line.starts_with("GET") {
            if let Some("/") = line
                .split_ascii_whitespace()
                .skip(1)
                .next() {
                    status_code = 200;
                }
        }
        _data.extend_from_slice(line.as_bytes());
    }
    write!(stream, "HTTP/1.1 {}\r\n\r\n", status_code.as_msg().unwrap())?;
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

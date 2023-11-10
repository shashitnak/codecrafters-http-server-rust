use std::{net::{TcpListener, TcpStream}, thread, io::{self, Write, BufRead, BufReader}, ops::Deref, fmt::{Debug, Display}, collections::HashMap};

trait HttpStatus: Sized
where
    u32: TryFrom<Self>,
{
    fn as_msg(self) -> Option<&'static str> {
        let status_code: u32 = self
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
    u32: TryFrom<T> {}

struct HttpResponse<'a> {
    status_code: u32,
    headers: Vec<(&'static str, &'a dyn Display)>,
    body: &'a dyn Deref<Target=[u8]>
}

impl<'a> HttpResponse<'a> {
    fn write_to_writer(&self, mut writer: impl Write) -> io::Result<()> {
        write!(writer, "HTTP/1.1 {}\r\n", self.status_code.as_msg().unwrap())?;

        for (key, value) in self.headers.iter() {
            write!(writer, "{key}: {value}\r\n")?;
        }

        write!(writer, "\r\n")?;
        writer.write_all(&self.body)?;
        write!(writer, "\r\n")?;

        Ok(())
    }
}

fn handle_client(stream: TcpStream) -> io::Result<()> {
    let mut _data = vec![];
    let mut reader = BufReader::new(&stream);
    let mut status_code = 404;
    let mut response = vec![];
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.as_str() == "\r\n" {
            break;
        }
        if line.starts_with("GET") {
            let path = line
                .split_ascii_whitespace()
                .skip(1)
                .next();
            match path {
                Some("/") => {
                    status_code = 200;
                }
                Some(path) if path.starts_with("/echo/") => {
                    status_code = 200;
                    response.extend_from_slice(&path.as_bytes()[6..]);
                },
                _ => {
                    status_code = 404;
                }
            }
        }
        _data.extend_from_slice(line.as_bytes());
    }

    let content_length = response.len();
    let headers: Vec<(&'static str, &dyn Display)> = vec![("Content-Type", &"text/plain"), ("Content-Length", &content_length)];
    HttpResponse {
        status_code,
        headers,
        body: &response
    }
        .write_to_writer(stream)?;
    
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

use std::{net::{TcpListener, TcpStream}, thread, io::{self, Write, BufRead, BufReader}, ops::Deref, fmt::Display, collections::HashMap, str::Utf8Error};


fn error(msg: impl AsRef<str>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.as_ref())
}

trait ParseUntil: Deref<Target=[u8]> + Sized {
    fn parse_until(&self, sep: &str) -> Result<(&str, &[u8]), Utf8Error>
    {
        let sep_bytes = sep.as_bytes();

        let mut i = 0;
        for (index, ch) in self.iter().enumerate() {
            if let Some(ch1) = sep_bytes.get(i) {
                if ch == ch1 {
                    i += 1;
                } else {
                    i = 0;
                }
            } else {
                return std::str::from_utf8(&self[0..index])
                    .map(|val| (val, &self[index..]))
            }
        }
        std::str::from_utf8(&self[..])
            .map(|val| (val, &self[self.len()..]))
    }
}

impl<T> ParseUntil for T
where
    T: Deref<Target=[u8]> + Sized
{}

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

struct HttpResponse {
    status_code: u32,
    headers: Vec<(&'static str, Box<dyn Display>)>,
    body: Vec<u8>
}

impl<'a> HttpResponse {
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

trait Responder<'a> {
    fn respond(&self, request: HttpRequest<'a>) -> Result<Vec<u8>, io::Error>;
}

struct ErrorResponder;
impl<'a> Responder<'a> for ErrorResponder {
    fn respond(&self, _request: HttpRequest<'a>) -> Result<Vec<u8>, io::Error> {
        Err(error("Not Found"))
    }
}

struct EmptyResponder;
impl<'a> Responder<'a> for EmptyResponder {
    fn respond(&self, _request: HttpRequest<'a>) -> Result<Vec<u8>, io::Error> {
        Ok(vec![])
    }
}

struct PathResponder;
impl<'a> Responder<'a> for PathResponder {
    fn respond(&self, request: HttpRequest<'a>) -> Result<Vec<u8>, io::Error> {
        if request.path.len() >= 6 {
            Ok(request.path.as_bytes()[6..].to_vec())
        } else {
            Err(error("Invalid Path"))
        }
    }
}

struct UserAgentResponder;
impl<'a> Responder<'a> for UserAgentResponder {
    fn respond(&self, request: HttpRequest<'a>) -> Result<Vec<u8>, io::Error> {
        let mut response = vec![];
        for (key, value) in request.headers.iter() {
            match key.as_str() {
                "User-Agent" => {
                    response.extend_from_slice(value.as_bytes());
                },
                _ => {}
            }
        }
        Ok(response)
    }
}

type Path<'a> = String;

#[derive(Clone)]
struct HttpRequest<'a> {
    path: Path<'a>,
    headers: HashMap<String, String>,
    body: &'a dyn BufRead
}

trait ReadHttpRequest<'a>: BufRead + Sized {
    fn read_http_request(&'a mut self) -> Result<HttpRequest<'a>, io::Error> {
        let mut header_head = String::new();
        self.read_line(&mut header_head)?;
        let words = header_head
            .trim()
            .split_ascii_whitespace();
        let path = words
            .skip(1)
            .next()
            .ok_or(error("No Path Found"))?
            .to_string();

        let mut headers = HashMap::new();
        loop {
            let mut line = String::new();
            self.read_line(&mut line)?;
            if let Some((key, val)) = line.trim().split_once(": ") {
                headers.insert(key.to_string(), val.to_string());
            } else {
                break
            }
        }

        Ok(HttpRequest {
            path,
            headers,
            body: self
        })
    }
}

impl<'a, R: BufRead> ReadHttpRequest<'a> for R {}

trait PathDispatch: AsRef<str> {
    fn dispatch(&self) -> Box<dyn Responder> {
        match self.as_ref() {
            "/" => Box::new(EmptyResponder),
            "/user-agent" => Box::new(UserAgentResponder),
            other if other.starts_with("/echo/")
                => Box::new(PathResponder),
            _ => Box::new(ErrorResponder)
        }
    }
}

impl<P> PathDispatch for P
where
    P: AsRef<str>
{}

impl<'a> From<HttpRequest<'a>> for HttpResponse {
    fn from(value: HttpRequest<'a>) -> Self {
        let body = value
            .clone()
            .path
            .dispatch()
            .respond(value)
            .ok();

        let (status_code, body) = match body {
            Some(body) => (200, body),
            None => (404, vec![])
        };

        let headers: Vec<(&'static str, Box<dyn Display>)> = vec![
            ("Content-Type", Box::new("text/plain")),
            ("Content-Length", Box::new(body.len()))
        ];


        HttpResponse {
            status_code,
            headers,
            body
        }
    }
}

fn handle_client(mut stream: TcpStream) -> io::Result<()> {
    let mut buf_reader = BufReader::new(&mut stream);
    let http_request = buf_reader
        .read_http_request()?;

    let response: HttpResponse = http_request
        .into();

    response
        .write_to_writer(stream)?;
    
    Ok(())
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("accepted new connection");
                // dummy change
                thread::spawn(|| handle_client(stream));
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

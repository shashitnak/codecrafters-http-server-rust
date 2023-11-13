use std::{net::{TcpListener, TcpStream}, thread, io::{self, Write, BufRead, BufReader, Read}, ops::Deref, fmt::Display, collections::HashMap, str::Utf8Error, path::{self, PathBuf}, any::Any, fs};
use std::sync::{Arc, Mutex};

const BUFFER_SIZE: usize = 4096;

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
            201 => "201 OK",
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

trait Status {
    fn status_code(self) -> u32;
}
struct NoStatus;
impl Status for NoStatus {
    fn status_code(self) -> u32 {
        200
    }
}

struct StatusCode(u32);
impl Status for StatusCode {
    fn status_code(self) -> u32 {
        self.0
    }
}

trait Header {
    fn header(self) -> Vec<(&'static str, Box<dyn Display>)>;
}
struct NoHeaders;
impl Header for NoHeaders {
    fn header(self) -> Vec<(&'static str, Box<dyn Display>)> {
        vec![]
    }
}

struct Headers(Vec<(&'static str, Box<dyn Display>)>);
impl Header for Headers {
    fn header(self) -> Vec<(&'static str, Box<dyn Display>)> {
        self.0
    }
}

trait HttpBody {
    fn body(self) -> Vec<u8>;
}
struct NoBody;
impl HttpBody for NoBody {
    fn body(self) -> Vec<u8> {
        vec![]
    }
}
struct Body(Vec<u8>);
impl HttpBody for Body {
    fn body(self) -> Vec<u8> {
        self.0
    }
}

struct HttpResponseBuilder<S: Status, H: Header, B: HttpBody> {
    status_code: S,
    headers: H,
    body: B
}

impl HttpResponseBuilder<NoStatus, NoHeaders, NoBody> {
    fn new() -> Self {
        Self {
            status_code: NoStatus,
            headers: NoHeaders,
            body: NoBody
        }
    }
}

impl<H: Header, B: HttpBody> HttpResponseBuilder<NoStatus, H, B> {
    fn status(self, status_code: u32) -> HttpResponseBuilder<StatusCode, H, B> {
        HttpResponseBuilder {
            status_code: StatusCode(status_code),
            headers: self.headers,
            body: self.body
        }
    }
}

impl<S: Status, B: HttpBody> HttpResponseBuilder<S, NoHeaders, B> {
    fn header(self, key: &'static str, val: impl Display + 'static) -> HttpResponseBuilder<S, Headers, B> {
        HttpResponseBuilder {
            status_code: self.status_code,
            headers: Headers(vec![]),
            body: self.body
        }
        .header(key, val)
    }
}

impl<S: Status, B: HttpBody> HttpResponseBuilder<S, Headers, B> {
    fn header(mut self, key: &'static str, val: impl Display + 'static) -> HttpResponseBuilder<S, Headers, B> {
        self.headers.0.push((key, Box::new(val)));
        self
    }
}

impl<S: Status, H: Header> HttpResponseBuilder<S, H, NoBody> {
    fn body(self, body: Vec<u8>) -> HttpResponseBuilder<S, H, Body> {
        HttpResponseBuilder {
            status_code: self.status_code,
            headers: self.headers,
            body: Body(body)
        }
    }
}

impl<S: Status, H: Header, B: HttpBody> HttpResponseBuilder<S, H, B> {
    fn into_http_response(self) -> HttpResponse {
        let mut headers = self.headers.header();
        let body = self.body.body();
        headers.push(("Content-Length", Box::new(body.len())));
        HttpResponse {
            status_code: self.status_code.status_code(),
            headers,
            body
        }
    }
}

impl HttpResponse {
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

    fn body(body: Vec<u8>) -> HttpResponse {
        let content_length = body.len();

        HttpResponse {
            status_code: 200,
            headers: vec![("Content-Length", Box::new(content_length))],
            body
        }
    }
}

trait Responder {
    fn respond(&self, request: HttpRequest) -> Result<HttpResponse, io::Error>;
}

struct ErrorResponder;
impl Responder for ErrorResponder {
    fn respond(&self, _request: HttpRequest) -> Result<HttpResponse, io::Error> {
        Err(error("Not Found"))
    }
}

struct EmptyResponder;
impl Responder for EmptyResponder {
    fn respond(&self, _request: HttpRequest) -> Result<HttpResponse, io::Error> {
        Ok(HttpResponseBuilder::new()
            .body(vec![])
            .into_http_response())
    }
}

struct PathResponder;
impl Responder for PathResponder {
    fn respond(&self, request: HttpRequest) -> Result<HttpResponse, io::Error> {
        if request.path.len() >= 6 {
            let body = request.path.as_bytes()[6..].to_vec();
            Ok(HttpResponseBuilder::new()
                .header("Content-Type", "text/plain")
                .body(body)
                .into_http_response())
        } else {
            Err(error("Invalid Path"))
        }
    }
}

struct UserAgentResponder;
impl Responder for UserAgentResponder {
    fn respond(&self, request: HttpRequest) -> Result<HttpResponse, io::Error> {
        let mut response = vec![];
        for (key, value) in request.headers.iter() {
            match key.as_str() {
                "User-Agent" => {
                    response.extend_from_slice(value.as_bytes());
                },
                _ => {}
            }
        }
        Ok(HttpResponseBuilder::new()
            .body(response)
            .into_http_response())
    }
}

struct FileResponder {
    path: PathBuf
}

impl FileResponder {
    fn get_response(&self, file_path: PathBuf, request: HttpRequest) -> Result<HttpResponse, io::Error> {
        let mut data = vec![];
        fs::File::open(file_path)?
            .read_to_end(&mut data)?;

        Ok(HttpResponseBuilder::new()
            .header("Content-Type", Box::new("application/octet-stream"))
            .body(data)
            .into_http_response())
    }

    fn post_response(&self, file_path: PathBuf, mut request: HttpRequest) -> Result<HttpResponse, io::Error> {
        let mut writer = fs::File::create(file_path)?;
        let content_length = request
            .headers
            .get("Content-Length")
            .ok_or(error("No Content-Length Found for request"))?
            .parse::<usize>()
            .map_err(|err| error(err.to_string()))?;

        let mut to_read = content_length;
        let mut buffer = [0u8; BUFFER_SIZE];

        while to_read > 0 {
            let bytes_to_read = std::cmp::min(BUFFER_SIZE, to_read);
            request.body.read_exact(&mut buffer[0..bytes_to_read])?;
            writer.write_all(&buffer[0..bytes_to_read])?;
            to_read -= bytes_to_read;
        }

        Ok(HttpResponseBuilder::new()
            .status(201)
            .into_http_response())
    }
}

impl Responder for FileResponder {
    fn respond(&self, request: HttpRequest) -> Result<HttpResponse, io::Error> {
        let filename = std::str::from_utf8(&request.path.as_bytes()[7..])
            .map_err(|err| error(format!("{err}")))?;
        let file_path = self.path.join(filename);

        match request.method {
            HttpRequestMethod::Get => self.get_response(file_path, request),
            HttpRequestMethod::Post => self.post_response(file_path, request)
        }
    }
}

type Path = String;

#[derive(Clone)]
enum HttpRequestMethod {
    Get,
    Post
}

struct HttpRequest<'a> {
    method: HttpRequestMethod,
    path: Path,
    headers: HashMap<String, String>,
    body: BufReader<&'a mut TcpStream>
}

fn read_http_request(mut reader: BufReader<&mut TcpStream>) -> Result<HttpRequest, io::Error> {
    let mut header_head = String::new();
    reader.read_line(&mut header_head)?;
    let mut words = header_head
        .trim()
        .split_ascii_whitespace();
    let method = words
        .next()
        .ok_or(error("No HttpMethod Found"))?;

    let method = match method.to_lowercase().as_str() {
        "get" => HttpRequestMethod::Get,
        "post" => HttpRequestMethod::Post,
        val => Err(error(format!("Invalid HttpMethod Found: {val}")))?
    };

    let path = words
        .next()
        .ok_or(error("No Path Found"))?
        .to_string();

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if let Some((key, val)) = line.trim().split_once(": ") {
            headers.insert(key.to_string(), val.to_string());
        } else {
            break
        }
    }

    Ok(HttpRequest {
        method,
        path,
        headers,
        body: reader
    })
}

trait PathDispatch: AsRef<str> {
    fn dispatch(&self, data: Arc<Data>) -> Result<Box<dyn Responder>, io::Error> {
        Ok(match self.as_ref() {
            "/" => Box::new(EmptyResponder),
            "/user-agent" => Box::new(UserAgentResponder),
            other if other.starts_with("/echo/")
                => Box::new(PathResponder),
            other if other.starts_with("/files/")
                => {
                    if let Some(path) = data.get::<PathBuf>() {
                        Box::new(FileResponder { path: path.clone() })
                    } else {
                        Err(error("No Directory provided"))?
                    }
                },
            _ => Box::new(ErrorResponder)
        })
    }
}

impl<P> PathDispatch for P
where
    P: AsRef<str>
{}




trait MyAny: Send + Sized + Clone {
    fn downcast<T>(&self) -> &T;
}

struct Data {
    datas: Vec<Arc<dyn Any + Send + Sync>>
}

impl Data {
    fn new() -> Self {
        Data { datas: vec![] }
    }

    fn insert<T: Any + Send + Sync>(&mut self, val: T) {
        self.datas.push(Arc::new(val));
    }

    fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self
            .datas
            .iter()
            .fold(None, |res, data| {
                match res {
                    res@Some(_) => res,
                    None => data.downcast_ref::<T>()
                }
            })
    }
}

impl<'a> TryFrom<(HttpRequest<'a>, Arc<Data>)> for HttpResponse {
    type Error = io::Error;

    fn try_from((value, data): (HttpRequest<'a>, Arc<Data>)) -> Result<Self, Self::Error> {
        value
            .path
            .clone()
            .dispatch(data)?
            .respond(value)
    }
}


fn handle_client(mut stream: TcpStream, data: Arc<Data>) -> io::Result<()> {
    let buf_reader = BufReader::new(&mut stream);
    let http_request = read_http_request(buf_reader)?;

    let response: HttpResponse = (http_request, data)
        .try_into()
        .unwrap_or(HttpResponseBuilder::new()
            .status(404)
            .into_http_response()
        );

    response
        .write_to_writer(stream)?;
    
    Ok(())
}

fn main() {
    let mut data = Data::new();

    let args1 = std::env::args();
    let args2 = std::env::args().skip(1);
    for (arg1, arg2) in args1.zip(args2) {
        if arg1.as_str() == "--directory" {
            data.insert(path::Path::new(&arg2).to_path_buf());
        }
    }

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();
    
    let data = Arc::new(data);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("accepted new connection");
                let data = data.clone();
                thread::spawn(move || {
                    if let Err(err) = handle_client(stream, data) {
                        println!("handle_client failed with: {err:?}");
                    }
                });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

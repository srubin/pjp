use std::{
    borrow::BorrowMut,
    collections::HashMap,
    io::{prelude::*, BufReader},
    net::TcpStream,
    str::FromStr,
};

use log::{debug, info};
use serde::Serialize;

pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

pub struct HttpRequest {
    pub method: HttpMethod,
    pub path: String,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

pub enum HttpResponseCode {
    Ok,
    NotFound,
    InternalServerError,
    BadRequest,
}

pub struct HttpResponse {
    stream: TcpStream,
    pub headers: HashMap<String, String>,
    pub response_code: HttpResponseCode,
    json_body: Option<String>,
    sent_response: bool,
}

impl FromStr for HttpMethod {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PATCH" => Ok(HttpMethod::Patch),
            "PUT" => Ok(HttpMethod::Put),
            "DELETE" => Ok(HttpMethod::Delete),
            _ => Err(()),
        }
    }
}

impl TryFrom<&mut TcpStream> for HttpRequest {
    type Error = ();

    fn try_from(stream: &mut TcpStream) -> Result<Self, Self::Error> {
        let mut buf_reader = BufReader::new(std::io::Read::by_ref(stream));

        let mut http_request_lines = Vec::new();
        loop {
            let mut line = String::new();
            let bytes_read = buf_reader.read_line(&mut line).unwrap();
            line = line.trim().to_string();
            if line.is_empty() || bytes_read == 0 {
                break;
            }
            http_request_lines.push(line);
        }

        let mut req = HttpRequest {
            method: HttpMethod::Get,
            path: String::from(""),
            version: String::from(""),
            headers: HashMap::new(),
            body: String::from(""),
        };

        info!("http request: {:?}", http_request_lines);

        for (i, line) in http_request_lines.iter().enumerate() {
            if i == 0 {
                let parts: Vec<&str> = line.split(" ").collect();
                req.method = HttpMethod::from_str(parts[0])?;
                req.path = String::from(parts[1]);
                req.version = String::from(parts[2]);
            } else {
                let parts: Vec<&str> = line.split(": ").collect();
                req.headers.insert(
                    String::from(parts[0]).to_lowercase(),
                    String::from(parts[1]),
                );
            }
        }

        // read the body
        if let Some(header) = req.headers.get("content-length") {
            if let Ok(content_length) = header.parse::<usize>() {
                let mut buf = vec![0; content_length];
                buf_reader.read_exact(&mut buf).unwrap();
                req.body = String::from_utf8(buf).unwrap();
            }
        }

        debug!("http request body: {:?}", req.body);

        Ok(req)
    }
}

impl HttpResponse {
    pub fn new(stream: TcpStream) -> HttpResponse {
        HttpResponse {
            stream,
            headers: HashMap::new(),
            response_code: HttpResponseCode::Ok,
            json_body: None,
            sent_response: false,
        }
    }

    pub fn set_json<T>(&mut self, value: &T)
    where
        T: ?Sized + Serialize,
    {
        self.json_body = Some(serde_json::to_string(value).unwrap());
    }

    fn send_response(&mut self) {
        // TODO: error handing

        if self.sent_response {
            return;
        }

        let mut response = String::from("HTTP/1.1 ");

        response.push_str(match self.response_code {
            HttpResponseCode::Ok => "200 OK",
            HttpResponseCode::NotFound => "404 Not Found",
            HttpResponseCode::InternalServerError => "500 Internal Server Error",
            HttpResponseCode::BadRequest => "400 Bad Request",
        });

        response.push_str("\r\n");

        // encode json body if we have one
        if let Some(json_body) = &self.json_body {
            self.headers.insert(
                String::from("Content-Type"),
                String::from("application/json"),
            );
            self.headers
                .insert(String::from("Content-Length"), json_body.len().to_string());
        }

        for (key, value) in &self.headers {
            response.push_str(key);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }

        response.push_str("\r\n");

        if let Some(json_body) = &self.json_body {
            response.push_str(json_body);
        }

        self.stream.write_all(response.as_bytes()).unwrap();

        self.sent_response = true;
    }

    pub fn prep_sse(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.headers.insert(
            String::from("Content-Type"),
            String::from("text/event-stream"),
        );
        self.send_response();
        Ok(())
    }

    pub fn send_sse(
        &mut self,
        id: u32,
        event: &str,
        data: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let response = format!("id: {}\nevent: {}\ndata: {}\n\n", id, event, data);
        self.stream.write_all(response.as_bytes())?;
        Ok(())
    }
}

impl Drop for HttpResponse {
    fn drop(&mut self) {
        self.send_response();
    }
}

pub fn handle_connection(mut stream: TcpStream) -> (Result<HttpRequest, ()>, HttpResponse) {
    let req = HttpRequest::try_from(stream.borrow_mut());
    let res: HttpResponse = HttpResponse::new(stream);
    (req, res)
}

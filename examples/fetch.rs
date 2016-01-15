use std::process::exit;

extern crate nix;
extern crate rotor;
extern crate rotor_stream;
extern crate argparse;
extern crate time;
extern crate httparse;

use std::cmp::max;
use std::str::from_utf8;
use std::net::ToSocketAddrs;
use std::io::{stdout, stderr, Write};

use time::{SteadyTime, Duration};
use argparse::{ArgumentParser, Store};
use rotor::mio::tcp::{TcpStream};
use rotor_stream::{Stream, Transport, Protocol, Request, Expectation as E};
use rotor::{Scope};


struct Context;

enum Http {
    SendRequest(String),
    ReadHeaders,
    ReadBody,
}


impl<'a> Protocol<Context, TcpStream> for Http {
    type Seed = (String, String);
    fn create((host, path): Self::Seed, _sock: &mut TcpStream,
        _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        // Wait socket to become writable
        let req = format!(concat!(
                "GET {} HTTP/1.1\r\n",
                "Host: {}\r\n",
                "\r\n"),
            path, host);
        println!("----- Request -----");
        print!("{}", req);
        Some((Http::SendRequest(req), E::Flush(0),
            SteadyTime::now() + Duration::seconds(10)))
    }
    fn bytes_flushed(self, transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        match self {
            Http::SendRequest(val) => {
                transport.output().extend(val.as_bytes());
                Some((Http::ReadHeaders, E::Delimiter(0, b"\r\n\r\n", 4096),
                    SteadyTime::now() + Duration::seconds(10)))
            }
            _ => unreachable!(),
        }
    }
    fn bytes_read(self, transport: &mut Transport<TcpStream>,
                  end: usize, _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        match self {
            Http::SendRequest(_) => unreachable!(),
            Http::ReadHeaders => {
                let mut headers = [httparse::EMPTY_HEADER; 16];
                {
                    let mut req = httparse::Response::new(&mut headers);
                    let buf = &transport.input()[..end+4];
                    req.parse(buf).unwrap();
                }
                let mut clen = 0;
                println!("----- Headers -----");
                for header in headers.iter() {
                    println!("{}: {}",
                        header.name,
                        from_utf8(header.value).unwrap());
                    let hlower = header.name.to_lowercase();
                    if &hlower[..] == "content-length" {
                        if let Some(val) = from_utf8(header.value).ok()
                                           .and_then(|x| x.parse().ok()) {
                            clen = val;
                            break;
                        }
                    }
                }
                println!("----- Body [{}] -----", clen);
                transport.input().consume(end + 4);
                Some((Http::ReadBody, E::Bytes(clen),
                    SteadyTime::now()  + Duration::seconds(max(10, clen as i64))))
            }
            Http::ReadBody => {
                let newline = {
                    let data = &transport.input()[..];
                    data.len() > 0 && &data[data.len()-1..] != b"\n"
                };
                transport.input().write_to(&mut stdout()).ok();
                if newline {
                    println!("");
                }
                println!("----- Done -----");
                None
            }
        }
    }
    fn timeout(self, _transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        writeln!(&mut stderr(), "Timeout reached").ok();
        return None;
    }

    /// Message received (from the main loop)
    fn wakeup(self, _transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        unreachable!();
    }
}

fn main() {
    let mut url = "".to_string();
    {
        let mut ap = ArgumentParser::new();
        ap.set_description("
            Fetches some url. Only http, and only fixed size pages are
            supported (i.e. where Content-Length is set)
            ");
        ap.refer(&mut url).add_argument("url", Store, "
            The url to fetch.");
        ap.parse_args_or_exit();
    }
    if !url.starts_with("http://") {
        println!("Url should start with http://");
        exit(1);
    }
    let (host, path) = if let Some(idx) = url[7..].find('/') {
        (&url[7..idx+7], &url[idx+7..])
    } else {
        (&url[7..], "/")
    };
    println!("Host: {} (port: 80), path: {}", host, path);

    let mut event_loop = rotor::EventLoop::new().unwrap();
    let mut handler = rotor::Handler::new(Context, &mut event_loop);

    let sock = TcpStream::connect(
        // Any better way for current stable rust?
        &(host, 80).to_socket_addrs().unwrap().next().unwrap()).unwrap();

    let conn = handler.add_machine_with(&mut event_loop, |scope| {
        Stream::<Context, TcpStream, Http>::new(
            sock, (host.to_string(), path.to_string()),
            scope)
    });
    assert!(conn.is_ok());
    event_loop.run(&mut handler).unwrap();
}

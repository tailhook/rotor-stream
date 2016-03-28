use std::process::exit;

extern crate nix;
extern crate rotor;
extern crate rotor_stream;
extern crate argparse;
extern crate httparse;

use std::cmp::max;
use std::str::from_utf8;
use std::net::ToSocketAddrs;
use std::io::{stdout, stderr, Write};
use std::time::Duration;

use argparse::{ArgumentParser, Store};
use rotor::mio::tcp::{TcpStream};
use rotor_stream::{Stream, Transport, Protocol, Intent, Exception};
use rotor::{Scope};


struct Context;

enum Http {
    SendRequest(String),
    ReadHeaders,
    ReadBody,
}


impl<'a> Protocol for Http {
    type Context = Context;
    type Socket = TcpStream;
    type Seed = (String, String);
    fn create((host, path): Self::Seed, _sock: &mut TcpStream,
        scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        // Wait socket to become writable
        let req = format!(concat!(
                "GET {} HTTP/1.1\r\n",
                "Host: {}\r\n",
                "\r\n"),
            path, host);
        println!("----- Request -----");
        print!("{}", req);
        Intent::of(Http::SendRequest(req)).expect_flush()
            .deadline(scope.now() + Duration::new(10, 0))
    }
    fn bytes_flushed(self, transport: &mut Transport<TcpStream>,
        scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        match self {
            Http::SendRequest(val) => {
                transport.output().extend(val.as_bytes());
                Intent::of(Http::ReadHeaders)
                    .expect_delimiter(b"\r\n\r\n", 4096)
                    .deadline(scope.now() + Duration::new(10, 0))
            }
            _ => unreachable!(),
        }
    }
    fn bytes_read(self, transport: &mut Transport<TcpStream>,
                  end: usize, scope: &mut Scope<Context>)
        -> Intent<Self>
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
                Intent::of(Http::ReadBody).expect_bytes(clen)
                    .deadline(scope.now() +
                              Duration::new(max(10, clen as u64), 0))
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
                Intent::done()
            }
        }
    }
    fn timeout(self, _transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        writeln!(&mut stderr(), "Timeout reached").ok();
        Intent::done()
    }

    /// Message received (from the main loop)
    fn wakeup(self, _transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        unreachable!();
    }

    fn exception(self, _transport: &mut Transport<Self::Socket>,
        reason: Exception, scope: &mut Scope<Self::Context>)
        -> Intent<Self>
    {
        writeln!(&mut stderr(), "Error when fetching data: {}", reason).ok();
        scope.shutdown_loop();
        Intent::error(Box::new(reason))
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

    let event_loop = rotor::Loop::new(&rotor::Config::new()).unwrap();

    let sock = TcpStream::connect(
        // Any better way for current stable rust?
        &(host, 80).to_socket_addrs().unwrap().next().unwrap()).unwrap();

    let mut loop_inst = event_loop.instantiate(Context);
    loop_inst.add_machine_with(|scope| {
        Stream::<Http>::new(
            sock, (host.to_string(), path.to_string()),
            scope)
    }).unwrap();
    loop_inst.run().unwrap();
}

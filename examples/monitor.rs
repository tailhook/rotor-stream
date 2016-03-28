extern crate nix;
extern crate rotor;
extern crate rotor_stream;
extern crate argparse;
extern crate httparse;
extern crate env_logger;

use std::cmp::max;
use std::str::from_utf8;
use std::net::ToSocketAddrs;
use std::io::{stdout, stderr, Write};
use std::time::Duration;
use std::error::Error;

use rotor::mio::tcp::{TcpStream};
use rotor_stream::{Persistent, Transport, Protocol, Intent, Exception};
use rotor::{Scope};


struct Context;

#[derive(Debug)]
enum Http {
    SendRequest(String),
    ReadHeaders(String),
    ReadBody(String),
    Sleep(String),
}


impl<'a> Protocol for Http {
    type Context = Context;
    type Socket = TcpStream;
    type Seed = (String, String);
    fn create((host, path): Self::Seed, _sock: &mut TcpStream,
        scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        println!("create");
        // Wait socket to become writable
        let req = format!(concat!(
                "GET {} HTTP/1.1\r\n",
                "Host: {}\r\n",
                "User-Agent: curl\r\n", // required for timeapi.org
                "\r\n"),
            path, host);
        Intent::of(Http::SendRequest(req)).expect_flush()
            .deadline(scope.now() + Duration::new(10, 0))
    }
    fn bytes_flushed(self, transport: &mut Transport<TcpStream>,
        scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        println!("bytes_flushed {:?}", self);
        match self {
            Http::SendRequest(s) => {
                transport.output().extend(s.as_bytes());
                Intent::of(Http::ReadHeaders(s))
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
        println!("bytes_read {:?}", self);
        match self {
            Http::Sleep(x) => Intent::of(Http::Sleep(x)).sleep()
                              // Imprecise, still good enough
                              .deadline(scope.now() + Duration::new(5, 0)),
            Http::SendRequest(_) => unreachable!(),
            Http::ReadHeaders(s) => {
                let mut headers = [httparse::EMPTY_HEADER; 16];
                {
                    let mut req = httparse::Response::new(&mut headers);
                    let buf = &transport.input()[..end+4];
                    req.parse(buf).unwrap();
                }
                let mut clen = 0;
                for header in headers.iter() {
                    let hlower = header.name.to_lowercase();
                    if &hlower[..] == "content-length" {
                        if let Some(val) = from_utf8(header.value).ok()
                                           .and_then(|x| x.parse().ok()) {
                            clen = val;
                            break;
                        }
                    }
                }
                transport.input().consume(end + 4);
                Intent::of(Http::ReadBody(s)).expect_bytes(clen)
                    .deadline(scope.now() + Duration::new(
                        max(10, clen as u64), 0))
            }
            Http::ReadBody(s) => {
                let newline = {
                    let data = &transport.input()[..];
                    data.len() > 0 && &data[data.len()-1..] != b"\n"
                };
                transport.input().write_to(&mut stdout()).ok();
                if newline {
                    println!("");
                }
                println!("at {:?} wait for {:?} ",
                    scope.now(), scope.now() + Duration::new(10, 0));
                Intent::of(Http::Sleep(s)).sleep()
                    .deadline(scope.now() + Duration::new(10, 0))
            }
        }
    }
    fn timeout(self, _transport: &mut Transport<TcpStream>,
        scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        println!("timeout {:?} at {:?}", self, scope.now());
        match self {
            Http::Sleep(x) => {
                Intent::of(Http::SendRequest(x)).expect_flush()
                    .deadline(scope.now() + Duration::new(10, 0))
            }
            _ => {
                writeln!(&mut stderr(), "Timeout reached").ok();
                Intent::done()
            }
        }
    }

    /// Message received (from the main loop)
    fn wakeup(self, _transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        unreachable!("wakeup");
    }

    fn exception(self, _transport: &mut Transport<Self::Socket>,
        reason: Exception, _scope: &mut Scope<Self::Context>)
        -> Intent<Self>
    {
        writeln!(&mut stderr(), "Error when fetching data: {}", reason).ok();
        Intent::done()
    }
    fn fatal(self, reason: Exception, _scope: &mut Scope<Self::Context>)
        -> Option<Box<Error>>
    {
        writeln!(&mut stderr(), "Error when fetching data: {}", reason).ok();
        None
    }
}

fn main() {
    env_logger::init().expect("Can't initialize logging");
    let event_loop = rotor::Loop::new(&rotor::Config::new()).unwrap();

    let mut loop_inst = event_loop.instantiate(Context);
    loop_inst.add_machine_with(|scope| {
        Persistent::<Http>::connect(scope,
            ("www.timeapi.org", 80).to_socket_addrs().unwrap()
                .collect::<Vec<_>>()[0],
            ("www.timeapi.org".to_string(), "/utc/now.json".to_string()))
    }).unwrap();
    loop_inst.run().unwrap();
}

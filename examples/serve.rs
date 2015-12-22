extern crate mio;
extern crate time;
extern crate rotor;
extern crate rotor_stream;

use std::io::{Write};

use mio::tcp::{TcpListener, TcpStream};
use time::{SteadyTime, Duration};
use rotor::{Scope};
use rotor_stream::{Accept, Stream, Protocol, Request, Transport};
use rotor_stream::{Expectation as E};


struct Context;

enum Http {
    ReadHeaders,
    SendResponse,
}

impl Protocol<Context, TcpStream> for Http {
    type Seed = ();
    fn create(_seed: (), _sock: &mut TcpStream, _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        Some((Http::ReadHeaders, E::Delimiter(b"\r\n\r\n", 4096),
            SteadyTime::now() + Duration::seconds(10)))
    }
    fn bytes_read(self, transport: &mut Transport,
                  _end: usize, _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        transport.output().write_all(concat!(
            "HTTP/1.0 200 OK\r\n",
            "Server: rotor-stream-example-serve\r\n",
            "Connection: close\r\n",
            "Content-Length: 14\r\n",
            "\r\n",
            "Hello World!\r\n",
        ).as_bytes()).unwrap();
        Some((Http::SendResponse, E::Flush(0),
            SteadyTime::now() + Duration::seconds(10)))
    }
    fn bytes_flushed(self, _transport: &mut Transport,
                     _scope: &mut Scope<Context>)
        -> Request<Self>
    {
        // TODO(tailhook) or maybe start over?
        None
    }
    fn timeout(self, _scope: &mut Scope<Context>) -> Request<Self> {
        println!("Timeout");
        None
    }

    fn wakeup(self, _scope: &mut Scope<Context>) -> Request<Self> {
        unreachable!();
    }
}

fn main() {
    let mut event_loop = mio::EventLoop::new().unwrap();
    let mut handler = rotor::Handler::new(Context, &mut event_loop);
    let lst = TcpListener::bind(&"127.0.0.1:3000".parse().unwrap()).unwrap();
    let ok = handler.add_machine_with(&mut event_loop, |scope| {
        Accept::<TcpListener, TcpStream, Stream<Context, _, Http>>::new(lst, scope)
    }).is_ok();
    assert!(ok);
    event_loop.run(&mut handler).unwrap();
}

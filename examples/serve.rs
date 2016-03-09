extern crate rotor;
extern crate rotor_stream;

use std::io::{Write};
use std::time::Duration;

use rotor::mio::tcp::{TcpListener, TcpStream};
use rotor::{Scope};
use rotor_stream::{Accept, Stream, Protocol, Intent, Transport};


struct Context;

enum Http {
    ReadHeaders,
    SendResponse,
}

impl Protocol for Http {
    type Context = Context;
    type Socket = TcpStream;
    type Seed = ();
    fn create(_seed: (), _sock: &mut TcpStream, scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        Intent::of(Http::ReadHeaders).expect_delimiter(b"\r\n\r\n", 4096)
            .deadline(scope.now() + Duration::new(10, 0))
    }
    fn bytes_read(self, transport: &mut Transport<TcpStream>,
                  _end: usize, scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        println!("Request from {:?}", transport.socket().local_addr());
        transport.output().write_all(concat!(
            "HTTP/1.0 200 OK\r\n",
            "Server: rotor-stream-example-serve\r\n",
            "Connection: close\r\n",
            "Content-Length: 14\r\n",
            "\r\n",
            "Hello World!\r\n",
        ).as_bytes()).unwrap();
        Intent::of(Http::SendResponse).expect_flush()
            .deadline(scope.now() + Duration::new(10, 0))
    }
    fn bytes_flushed(self, _transport: &mut Transport<TcpStream>,
                     _scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        // TODO(tailhook) or maybe start over?
        Intent::done()
    }
    fn timeout(self, _transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        println!("Timeout");
        Intent::done()
    }

    fn wakeup(self, _transport: &mut Transport<TcpStream>,
        _scope: &mut Scope<Context>)
        -> Intent<Self>
    {
        unreachable!();
    }
}

fn main() {
    let mut event_loop = rotor::Loop::new(&rotor::Config::new()).unwrap();
    let lst = TcpListener::bind(&"127.0.0.1:3000".parse().unwrap()).unwrap();
    let ok = event_loop.add_machine_with(|scope| {
        Accept::<Stream<Http>, _>::new(lst, (), scope)
    }).is_ok();
    assert!(ok);
    event_loop.run(Context).unwrap();
}

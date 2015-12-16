use time::SteadyTime;
use rotor::Scope;

use {Transport, Request, StreamSocket};


pub enum Expectation {
    Bytes(usize),
    Delimiter(&'static str),
    Flush(usize),
    Sleep,
}

pub trait Protocol<C, S: StreamSocket>: Sized {
    /// Starting the protocol (e.g. accepted a socket)
    fn new(self, sock: &mut S) -> Request<Self>;

    /// Starting the protocol (e.g. accepted a socket)
    fn start(self, scope: &mut Scope<C>) -> Request<Self>;

    /// The action WaitBytes or WaitDelimiter is complete
    fn bytes_read(self, transport: &mut Transport,
                  end: usize, scope: &mut Scope<C>)
        -> Request<Self>;

    /// The action Flush is complete
    fn bytes_flushed(self, scope: &mut Scope<C>) -> Request<Self>;

    /// Timeout happened, which means either deadline reached in
    /// Bytes, Delimiter, Flush. Or Sleep has passed.
    fn timeout(self, scope: &mut Scope<C>) -> Request<Self>;

    /// Message received (from the main loop)
    fn wakeup(self, scope: &mut Scope<C>) -> Request<Self>;
}

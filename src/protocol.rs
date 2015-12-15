use time::SteadyTime;
use rotor::Scope;

use {Transport};


pub enum Expectation<M: Sized> {
    Bytes(M, usize, SteadyTime),
    Delimiter(M, &'static str, SteadyTime),
    Flush(M, usize, SteadyTime),
    Sleep(M, SteadyTime),
}

pub trait Protocol<C>: Sized {
    /// Starting the protocol (e.g. accepted a socket)
    fn start(self, scope: &mut Scope<C>) -> Expectation<Self>;

    /// The action WaitBytes or WaitDelimiter is complete
    fn bytes_read(self, transport: &mut Transport, scope: &mut Scope<C>)
        -> Expectation<Self>;

    /// The action Flush is complete
    fn bytes_flush(self, scope: &mut Scope<C>)
        -> Expectation<Self>;

    /// Timeout happened
    fn timeout(self, scope: &mut Scope<C>) -> Expectation<Self>;

    /// Message received (from the main loop)
    fn wakeup(self, scope: &mut Scope<C>) -> Expectation<Self>;
}

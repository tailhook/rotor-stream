extern crate netbuf;
extern crate memchr;
extern crate rotor;
extern crate time;
extern crate mio;
extern crate void;

mod substr;
mod transport;
mod protocol;
mod stream;

pub use protocol::{Protocol, Expectation};

use std::any::Any;
use std::marker::PhantomData;
use std::io::{Read, Write};
use mio::{Evented, Timeout};
use time::SteadyTime;

pub type Deadline = SteadyTime;
pub type Request<M> = Option<(M, Expectation, Deadline)>;

// Any is needed to use Stream as a Seed for Machine
pub trait StreamSocket: Read + Write + Evented + Any {}

pub struct Transport<'a> {
    inbuf: &'a mut netbuf::Buf,
    outbuf: &'a mut netbuf::Buf,
}

pub struct Stream<C, S: StreamSocket, P: Protocol<C, S>> {
    socket: S,
    fsm: P,
    expectation: Expectation,
    deadline: Deadline,
    timeout: Timeout,
    phantom: PhantomData<*const C>,
}

struct StreamImpl<S: StreamSocket> {
    socket: S,
    expectation: Expectation,
    deadline: Deadline,
    timeout: Timeout,
}

impl<T> StreamSocket for T where T: Read, T: Write, T: Evented, T:Any {}

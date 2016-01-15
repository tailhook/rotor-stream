//! Stream abstraction for rotor
//!
//! Assumptions:
//!
//! 1. You read data by length-prefixed or fixed-string-delimited chunks rather
//!    than byte-by-byte
//! 2. Each chunk fits memory
//! 3. Your data stream is not entirely full-duplex: while you can read and
//!    write simultaneously, when you apply pushback (i.e. waiting for bytes to
//!    be flushed), you can't do reads [*]
//!
//! [*] This matches HTTP perfectly, and most bidirectional interactive
//!     workflows (including based on websockets). But for some cases may be
//!     hard to implement. One such case is when you need to generate some
//!     output stream (you can't buffer it), and have to parse input stream at
//!     the same time.

extern crate netbuf;
extern crate memchr;
extern crate rotor;
extern crate time;
extern crate void;
#[macro_use] extern crate quick_error;


mod substr;
mod transport;
mod protocol;
mod stream;
mod accept;

pub use protocol::{Protocol, Expectation, Exception};
pub use accept::{Accepted};

use std::any::Any;
use std::marker::PhantomData;
use std::io::{Read, Write};
use time::SteadyTime;

use rotor::{Machine, Evented, Timeout};
use rotor::mio::{TryAccept};

pub use netbuf::Buf;
pub type Deadline = SteadyTime;
pub type Request<M> = Option<(M, Expectation, Deadline)>;

// Any is needed to use Stream as a Seed for Machine
pub trait StreamSocket: Read + Write + Evented + Any {}

pub struct Transport<'a, S: StreamSocket> {
    sock: &'a mut S,
    inbuf: &'a mut Buf,
    outbuf: &'a mut Buf,
}

/// Socket acceptor State Machine
///
/// TODO(tailhook) Currently this panics when there is no slab space when
/// accepting a connection. This may be fixed by sleeping and retrying
pub enum Accept<A: TryAccept+Sized, M: Machine> {
    Server(A),
    Connection(M),
}

pub struct Stream<C, S: StreamSocket, P: Protocol<C, S>> {
    socket: S,
    fsm: P,
    expectation: Expectation,
    deadline: Deadline,
    timeout: Timeout,
    inbuf: Buf,
    outbuf: Buf,
    phantom: PhantomData<*const C>,
}

struct StreamImpl<S: StreamSocket> {
    socket: S,
    deadline: Deadline,
    timeout: Timeout,
    inbuf: Buf,
    outbuf: Buf,
}

impl<T> StreamSocket for T where T: Read, T: Write, T: Evented, T:Any {}

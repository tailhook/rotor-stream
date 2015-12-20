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
    inbuf: netbuf::Buf,
    outbuf: netbuf::Buf,
    phantom: PhantomData<*const C>,
}

struct StreamImpl<S: StreamSocket> {
    socket: S,
    deadline: Deadline,
    timeout: Timeout,
    inbuf: netbuf::Buf,
    outbuf: netbuf::Buf,
}

impl<T> StreamSocket for T where T: Read, T: Write, T: Evented, T:Any {}

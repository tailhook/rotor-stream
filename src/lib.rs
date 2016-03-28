//! Stream abstraction for rotor
//!
//! The crate provides:
//!
//! * Buffering for network sockets
//! * Simple abstractions like read N bytes, read until '\n'
//! * Persistent (auto-reconnecting) client connections
//! * Abstraction for accepting connection on server-side
//!
//! Assumptions for streams:
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
#[macro_use] extern crate log;
#[macro_use] extern crate quick_error;
#[cfg(feature="replaceable")] extern crate rotor_tools;


mod substr;
mod transport;
mod protocol;
mod stream;
mod accept;
mod persistent;
mod trait_impls;
mod intention;
mod extensions;
mod errors;

pub use protocol::{Protocol, Expectation, Exception};
pub use accept::{Accepted};
pub use persistent::{Persistent};
pub use errors::ProtocolStop;

use std::any::Any;
use std::io;
use std::io::{Read, Write};
use std::error::Error;

use rotor::{Evented, Time};
use rotor::mio::{TryAccept};

pub use netbuf::{Buf, MAX_BUF_SIZE};

// Any is needed to use Stream as a Seed for Machine
pub trait StreamSocket: Read + Write + Evented + SocketError + Sized + Any {}

/// Transport is thing that provides buffered I/O for stream sockets
///
/// This is usually passed in all the `Protocol` handler methods. But in
/// case you manipulate the transport by some external methods (like the
/// one stored in `Arc<Mutex<Stream>>` you may wish to use `Stream::transport`
/// or `Persistent::transport` methods to manipulate tranpsport. Just remember
/// to **wake up** the state machine after manipulating buffers of transport.
pub struct Transport<'a, S: StreamSocket> {
    sock: &'a mut S,
    inbuf: &'a mut Buf,
    outbuf: &'a mut Buf,
}

/// Socket acceptor State Machine
///
/// TODO(tailhook) Currently this panics when there is no slab space when
/// accepting a connection. This may be fixed by sleeping and retrying
pub enum Accept<M, A: TryAccept+Sized>
    where A::Output: StreamSocket,
          M: Accepted<Socket=A::Output>,
{
    Server(A, <M as Accepted>::Seed),
    Connection(M),
}

/// A main stream state machine abstaction
///
/// You may use the `Stream` directly. But it's recommented to either use
/// `Persistent` for client connections or `Accept` for server-side
/// connection processing.
#[derive(Debug)]
pub struct Stream<P: Protocol> {
    socket: P::Socket,
    fsm: P,
    expectation: Expectation,
    connected: bool,
    deadline: Option<Time>,
    inbuf: Buf,
    outbuf: Buf,
}

struct StreamImpl<S: StreamSocket> {
    socket: S,
    connected: bool,
    inbuf: Buf,
    outbuf: Buf,
}

pub trait ActiveStream: StreamSocket {
    type Address;
    fn connect(addr: &Self::Address) -> io::Result<Self>;
}

pub trait SocketError {
    fn take_socket_error(&self) -> io::Result<()>;
}

/// A structure that encapsulates a state machine and an expectation
///
/// It's usually built with a builder that starts with `Intent::of(machine)`.
///
/// Then you add an expectation:
///
/// ```ignore
/// Intent::of(machine).expect_delimiter("\n", 1024)
/// ```
///
/// And then you may add a timeout (in form of "deadline" or "absolute time"):
///
/// ```ignore
/// Intent::of(machine).expect_bytes(100)
///     .deadline(scope.now() + Duration::new(10, 0))
/// ```
///
#[derive(Debug)]
pub struct Intent<M>(Result<M, Option<Box<Error>>>, Expectation, Option<Time>);

/// A helper class returned from `Intent::of()`
///
/// See the documentation of `Intent` for guide
#[derive(Debug)]
pub struct IntentBuilder<M>(M);

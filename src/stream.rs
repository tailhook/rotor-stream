use std::io;
use std::error::Error;
use std::io::ErrorKind::{WouldBlock, BrokenPipe, WriteZero, ConnectionReset};

use rotor::{Response, Scope, Machine, EventSet, PollOpt, Time};
use rotor::void::{Void, unreachable};

use substr::find_substr;
use extensions::{ScopeExt, ResponseExt};
use {Expectation, Protocol, StreamSocket, Stream, StreamImpl};
use {Buf, Transport, Accepted, Exception, Intent};
use {ProtocolStop, SocketError};


#[derive(Debug)]
enum IoOp {
    Done,
    NoOp,
    Eos,
    Error(io::Error),
}

fn to_result<P: Protocol>(intent: Intent<P>)
    -> Result<(P, Expectation, Option<Time>), Option<Box<Error>>>
{
    match intent.0 {
        Ok(x) => Ok((x, intent.1, intent.2)),
        Err(e) => Err(e),
    }
}

impl<S: StreamSocket> StreamImpl<S> {
    fn transport(&mut self) -> Transport<S> {
        Transport {
            sock: &mut self.socket,
            inbuf: &mut self.inbuf,
            outbuf: &mut self.outbuf,
        }
    }
    fn action<M>(self, intent: Intent<M>,
        scope: &mut Scope<M::Context>)
        -> Response<Stream<M>, Void>
        where M: Protocol<Socket=S>
    {
        match self._action(intent, scope) {
            Ok(stream) => {
                let dline = stream.deadline;
                Response::ok(stream).deadline_opt(dline)
            }
            Err(Some(e)) => Response::error(e),
            Err(None) => Response::done(),
        }
    }
    fn _action<P>(mut self, intent: Intent<P>,
        scope: &mut Scope<P::Context>)
        -> Result<Stream<P>, Option<Box<Error>>>
        where P: Protocol<Socket=S>
    {
        use Expectation::*;
        let mut intent = try!(to_result(intent));
        let mut can_write = match self.write() {
            IoOp::Done => true,
            IoOp::NoOp => false,
            IoOp::Eos => {
                self.outbuf.remove_range(..);
                return Err(intent.0.fatal(
                    Exception::WriteError(io::Error::new(
                        WriteZero, "failed to write whole buffer")),
                    scope));
            }
            IoOp::Error(e) => {
                return Err(intent.0.fatal(
                    Exception::WriteError(e),
                    scope));
            }
        };
        'outer: loop {
            if can_write {
                can_write = match self.write() {
                    IoOp::Done => true,
                    IoOp::NoOp => false,
                    IoOp::Eos => {
                        self.outbuf.remove_range(..);
                        return Err(intent.0.fatal(
                            Exception::WriteError(io::Error::new(
                                WriteZero, "failed to write whole buffer")),
                            scope));
                    }
                    IoOp::Error(e) => {
                        self.outbuf.remove_range(..);
                        return Err(intent.0.fatal(
                            Exception::WriteError(e),
                            scope));
                    }
                };
            }
            match intent.1 {
                Bytes(num) => {
                    loop {
                        if self.inbuf.len() >= num {
                            intent = try!(to_result(intent.0.bytes_read(
                                &mut self.transport(),
                                num, scope)));
                            continue 'outer;
                        }
                        match self.read() {
                            IoOp::Done => {}
                            IoOp::NoOp => {
                                return Ok(Stream::compose(self, intent));
                            }
                            IoOp::Eos => {
                                intent = try!(to_result(intent.0.exception(
                                    &mut self.transport(),
                                    Exception::EndOfStream,
                                    scope)));
                                continue 'outer;
                            }
                            IoOp::Error(e) => {
                                intent = try!(to_result(intent.0.exception(
                                    &mut self.transport(),
                                    Exception::ReadError(e),
                                    scope)));
                                continue 'outer;
                            }
                        }
                    }
                }
                Delimiter(min, delim, max) => {
                    loop {
                        if self.inbuf.len() > min {
                            let opt = find_substr(&self.inbuf[min..], delim);
                            if let Some(num) = opt {
                                intent = try!(to_result(intent.0.bytes_read(
                                    &mut self.transport(),
                                    num, scope)));
                                continue 'outer;
                            }
                        }
                        if self.inbuf.len() > max {
                            intent = try!(to_result(intent.0.exception(
                                &mut self.transport(),
                                Exception::LimitReached,
                                scope)));
                            continue 'outer;
                        }
                        match self.read() {
                            IoOp::Done => {}
                            IoOp::NoOp => {
                                return Ok(Stream::compose(self, intent));
                            }
                            IoOp::Eos => {
                                intent = try!(to_result(intent.0.exception(
                                    &mut self.transport(),
                                    Exception::EndOfStream,
                                    scope)));
                                continue 'outer;
                            }
                            IoOp::Error(e) => {
                                intent = try!(to_result(intent.0.exception(
                                    &mut self.transport(),
                                    Exception::ReadError(e),
                                    scope)));
                                continue 'outer;
                            }
                        }
                    }
                }
                Flush(num) => {
                    if self.outbuf.len() <= num {
                        intent = try!(to_result(intent.0.bytes_flushed(
                            &mut self.transport(), scope)));
                    } else {
                        return Ok(Stream::compose(self, intent));
                    }
                }
                Sleep => {
                    return Ok(Stream::compose(self, intent));
                }
            }
        }
    }
    // Returns Ok(true) to if we have read something, does not loop for reading
    // because this might use whole memory, and we may parse and consume the
    // input instead of buffering it whole.
    fn read(&mut self) -> IoOp {
        match self.inbuf.read_from(&mut self.socket) {
            Ok(0) => IoOp::Eos,
            Ok(_) => IoOp::Done,
            Err(ref e) if e.kind() == BrokenPipe
                       || e.kind() == ConnectionReset
            => return IoOp::Eos,
            Err(ref e) if e.kind() == WouldBlock => IoOp::NoOp,
            Err(e) => IoOp::Error(e),
        }
    }
    fn write(&mut self) -> IoOp {
        loop {
            if self.outbuf.len() == 0 {
                return IoOp::Done;
            }
            match self.outbuf.write_to(&mut self.socket) {
                Ok(0) => return IoOp::Eos,
                Ok(_) => continue,
                Err(ref e) if e.kind() == BrokenPipe
                           || e.kind() == ConnectionReset
                => return IoOp::Eos,
                Err(ref e) if e.kind() == WouldBlock => return IoOp::NoOp,
                Err(e) => return IoOp::Error(e),
            }
        }
    }
}

impl<P: Protocol> Accepted for Stream<P>
    where <P as Protocol>::Seed: Clone
{
    type Seed = <P as Protocol>::Seed;
    type Socket = P::Socket;
    fn accepted(sock: P::Socket, seed: <P as Protocol>::Seed,
        scope: &mut Scope<Self::Context>)
        -> Response<Self, Void>
    {
        Self::new(sock, seed, scope)
    }
}

impl<P: Protocol> Stream<P> {
    /// Get a `Transport` object for the stream
    ///
    /// This method is only useful  if you want to manipulate buffers
    /// externally (like pushing to the buffer from another thread). Just be
    /// sure to **wake up** state machine after manipulating buffers.
    pub fn transport(&mut self) -> Transport<P::Socket> {
        Transport {
            sock: &mut self.socket,
            inbuf: &mut self.inbuf,
            outbuf: &mut self.outbuf,
        }
    }
    fn decompose(self) -> (P, Expectation, Option<Time>, StreamImpl<P::Socket>)
    {
        (self.fsm, self.expectation, self.deadline, StreamImpl {
            socket: self.socket,
            connected: self.connected,
            inbuf: self.inbuf,
            outbuf: self.outbuf,
        })
    }
    fn compose(implem: StreamImpl<P::Socket>,
        (fsm, exp, dline): (P, Expectation, Option<Time>))
        -> Stream<P>
    {
        Stream {
            fsm: fsm,
            socket: implem.socket,
            expectation: exp,
            connected: implem.connected,
            deadline: dline,
            inbuf: implem.inbuf,
            outbuf: implem.outbuf,
        }
    }
    pub fn new(mut sock: P::Socket, seed: P::Seed,
        scope: &mut Scope<P::Context>)
        -> Response<Self, Void>
    {
        // Always register everything in edge-triggered mode.
        // This allows to never reregister socket.
        //
        // The no-reregister strategy is not a goal (although, it's expected
        // to lower number of syscalls for many request-reply-like protocols)
        // but it allows to have single source of truth for
        // readable()/writable() mask (no duplication in kernel space)
        if let Err(e) = scope.register(&sock,
            EventSet::readable() | EventSet::writable(), PollOpt::edge())
        {
            // TODO(tailhook) wrap it to more clear error
            return Response::error(Box::new(e));
        }
        let Intent(m, exp, dline) = P::create(seed, &mut sock, scope);
        match m {
            Err(None) => Response::error(Box::new(ProtocolStop)),
            Err(Some(e)) => Response::error(e),
            Ok(m) => {
                Response::ok(Stream {
                    socket: sock,
                    expectation: exp,
                    connected: false,
                    deadline: dline,
                    fsm: m,
                    inbuf: Buf::new(),
                    outbuf: Buf::new(),
                }).deadline_opt(dline)
            }
        }
    }
    pub fn connected(mut sock: P::Socket, seed: P::Seed,
        scope: &mut Scope<P::Context>)
        -> Response<Self, Void>
    {
        // Always register everything in edge-triggered mode.
        // This allows to never reregister socket.
        //
        // The no-reregister strategy is not a goal (although, it's expected
        // to lower number of syscalls for many request-reply-like protocols)
        // but it allows to have single source of truth for
        // readable()/writable() mask (no duplication in kernel space)
        //
        // We reregister here, because we assume that higher level abstraction
        // has the socket already registered (perhaps `Persistent` machine)
        if let Err(e) = scope.reregister(&sock,
            EventSet::readable() | EventSet::writable(), PollOpt::edge())
        {
            // TODO(tailhook) wrap it to more clear error
            return Response::error(Box::new(e));
        }
        let Intent(m, exp, dline) = P::create(seed, &mut sock, scope);
        match m {
            Err(None) => Response::error(Box::new(ProtocolStop)),
            Err(Some(e)) => Response::error(e),
            Ok(m) => {
                Response::ok(Stream {
                    socket: sock,
                    expectation: exp,
                    connected: true,
                    deadline: dline,
                    fsm: m,
                    inbuf: Buf::new(),
                    outbuf: Buf::new(),
                }).deadline_opt(dline)
            }
        }
    }
}

impl<P: Protocol> Machine for Stream<P>
{
    type Context = P::Context;
    type Seed = Void;
    fn create(void: Void, _scope: &mut Scope<Self::Context>)
        -> Response<Self, Void>
    {
        unreachable(void);
    }
    fn ready(self, events: EventSet, scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        // TODO(tailhook) use `events` to optimize reading
        let (fsm, exp, dline, mut imp) = self.decompose();
        if !imp.connected && events.is_writable() {
            match imp.socket.take_socket_error() {
                Ok(()) => {}
                Err(e) => {
                    match fsm.fatal(Exception::ConnectError(e), scope) {
                        Some(e) => return Response::error(e),
                        None => return Response::done(),
                    }
                }
            }
            imp.connected = true;
        }
        imp.action(Intent(Ok(fsm), exp, dline), scope)
    }
    fn spawned(self, _scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        unreachable!();
    }
    fn timeout(self, scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        if scope.reached(self.deadline) {
            let (fsm, _exp, _dline, mut imp) = self.decompose();
            let res = fsm.timeout(&mut imp.transport(), scope);
            imp.action(res, scope)
        } else {
            // TODO(tailhook) in rotor 0.6 should be no spurious timeouts
            // anymore, but let's keep it until we remove Scope::timeout_ms()
            Response::ok(self)
        }
    }
    fn wakeup(self, scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        let (fsm, _exp, _dline, mut imp) = self.decompose();
        let res = fsm.wakeup(&mut imp.transport(), scope);
        imp.action(res, scope)
    }
}

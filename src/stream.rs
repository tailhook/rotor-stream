use std::fmt;
use std::io;
use std::error::Error;
use std::marker::PhantomData;
use std::io::ErrorKind::{WouldBlock, BrokenPipe, WriteZero};

use time::SteadyTime;
use rotor::{Response, Scope, Machine};
use mio::{EventSet, PollOpt};
use netbuf::Buf;
use void::{Void, unreachable};

use substr::find_substr;
use {Expectation, Protocol, StreamSocket, Stream, StreamImpl, Request};
use {Transport, Deadline, Accepted, Exception};


#[derive(Debug)]
enum IoOp {
    Done,
    NoOp,
    Eos,
    Error(io::Error),
}

impl<S: StreamSocket> StreamImpl<S> {
    fn transport(&mut self) -> Transport<S> {
        Transport {
            sock: &mut self.socket,
            inbuf: &mut self.inbuf,
            outbuf: &mut self.outbuf,
        }
    }
    fn _action<C, M>(mut self, req: Request<M>, scope: &mut Scope<C>)
        -> Result<Stream<C, S, M>, ()>
        where M: Protocol<C, S>,
              S: StreamSocket,
    {
        use Expectation::*;
        let mut req = try!(req.ok_or(()));
        let mut can_write = match self.write() {
            IoOp::Done => true,
            IoOp::NoOp => false,
            IoOp::Eos => {
                req = try!(req.0.exception(
                    &mut self.transport(),
                    Exception::WriteError(io::Error::new(
                        WriteZero, "failed to write whole buffer")),
                    scope).ok_or(()));
                self.outbuf.remove_range(..);
                false
            }
            IoOp::Error(e) => {
                req = try!(req.0.exception(
                    &mut self.transport(),
                    Exception::WriteError(e),
                    scope).ok_or(()));
                // TODO(tailhook) should we flush buffer here too?
                false
            }
        };
        'outer: loop {
            if can_write {
                can_write = match self.write() {
                    IoOp::Done => true,
                    IoOp::NoOp => false,
                    IoOp::Eos => {
                        req = try!(req.0.exception(
                            &mut self.transport(),
                            Exception::WriteError(io::Error::new(
                                WriteZero, "failed to write whole buffer")),
                            scope).ok_or(()));
                        self.outbuf.remove_range(..);
                        false
                    }
                    IoOp::Error(e) => {
                        req = try!(req.0.exception(
                            &mut self.transport(),
                            Exception::WriteError(e),
                            scope).ok_or(()));
                        // TODO(tailhook) should we flush buffer here too?
                        false
                    }
                };
            }
            match req.1 {
                Bytes(num) => {
                    loop {
                        if self.inbuf.len() >= num {
                            req = try!(req.0.bytes_read(&mut self.transport(),
                                num, scope).ok_or(()));
                            continue 'outer;
                        }
                        match self.read() {
                            IoOp::Done => {}
                            IoOp::NoOp => {
                                return Ok(Stream::compose(self, req, scope));
                            }
                            IoOp::Eos => {
                                req = try!(req.0.exception(
                                    &mut self.transport(),
                                    Exception::EndOfStream,
                                    scope).ok_or(()));
                                continue 'outer;
                            }
                            IoOp::Error(e) => {
                                req = try!(req.0.exception(
                                    &mut self.transport(),
                                    Exception::ReadError(e),
                                    scope).ok_or(()));
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
                                req = try!(req.0.bytes_read(
                                    &mut self.transport(),
                                    num, scope).ok_or(()));
                                continue 'outer;
                            }
                        }
                        if self.inbuf.len() > max {
                            return Err(());
                        }
                        match self.read() {
                            IoOp::Done => {}
                            IoOp::NoOp => {
                                return Ok(Stream::compose(self, req, scope));
                            }
                            IoOp::Eos => {
                                req = try!(req.0.exception(
                                    &mut self.transport(),
                                    Exception::EndOfStream,
                                    scope).ok_or(()));
                                continue 'outer;
                            }
                            IoOp::Error(e) => {
                                req = try!(req.0.exception(
                                    &mut self.transport(),
                                    Exception::ReadError(e),
                                    scope).ok_or(()));
                                continue 'outer;
                            }
                        }
                    }
                }
                Flush(num) => {
                    if self.outbuf.len() <= num {
                        req = try!(req.0.bytes_flushed(&mut self.transport(),
                            scope).ok_or(()));
                    } else {
                        return Ok(Stream::compose(self, req, scope));
                    }
                }
                Sleep => {
                    return Ok(Stream::compose(self, req, scope));
                }
            }
        }
    }
    fn action<C, M>(self, req: Request<M>, scope: &mut Scope<C>)
        -> Response<Stream<C, S, M>, Void>
        where M: Protocol<C, S>,
              S: StreamSocket
    {
        let old_timeout = self.timeout;
        match self._action(req, scope) {
            Ok(x) => Response::ok(x),
            Err(()) => {
                scope.clear_timeout(old_timeout);
                Response::done()
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
            Err(ref e) if e.kind() == BrokenPipe => IoOp::Eos,
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
                Err(ref e) if e.kind() == BrokenPipe => return IoOp::Eos,
                Err(ref e) if e.kind() == WouldBlock => return IoOp::NoOp,
                Err(e) => return IoOp::Error(e),
            }
        }
    }
}

impl<C, S, P> Accepted<C, S> for Stream<C, S, P>
    where S: StreamSocket, P: Protocol<C, S, Seed=()>
{
    fn accepted(sock: S, scope: &mut Scope<C>) -> Result<Self, Box<Error>> {
        Self::new(sock, (), scope)
    }
}

impl<C, S: StreamSocket, P: Protocol<C, S>> Stream<C, S, P> {
    fn decompose(self) -> (P, Expectation, StreamImpl<S>) {
        (self.fsm, self.expectation, StreamImpl {
            socket: self.socket,
            deadline: self.deadline,
            timeout: self.timeout,
            inbuf: self.inbuf,
            outbuf: self.outbuf,
        })
    }
    fn compose(implem: StreamImpl<S>,
        (fsm, exp, dline): (P, Expectation, Deadline),
        scope: &mut Scope<C>)
        -> Stream<C, S, P>
    {
        let mut timeout = implem.timeout;
        if dline != implem.deadline {
            scope.clear_timeout(timeout);
            // Assuming that we can always add timeout since we have just
            // cancelled one. It may be not true if timer is already expired
            // or timeout is too far in future. But I'm not sure that killing
            // state machine here is much better idea than panicking.
            timeout = scope.timeout_ms(
                (dline - SteadyTime::now()).num_milliseconds() as u64)
                // TODO(tailhook) can we process the error somehow?
                .expect("Can't replace timer");
        }
        Stream {
            fsm: fsm,
            socket: implem.socket,
            expectation: exp,
            deadline: dline,
            timeout: timeout,
            inbuf: implem.inbuf,
            outbuf: implem.outbuf,
            phantom: PhantomData,
        }
    }
    pub fn new(mut sock: S, seed: P::Seed, scope: &mut Scope<C>)
        -> Result<Self, Box<Error>>
    {
        // Always register everything in edge-triggered mode.
        // This allows to never reregister socket.
        //
        // The no-reregister strategy is not a goal (although, it's expected
        // to lower number of syscalls for many request-reply-like protocols)
        // but it allows to have single source of truth for
        // readable()/writable() mask (no duplication in kernel space)
        try!(scope.register(&sock,
            EventSet::readable() | EventSet::writable(), PollOpt::edge()));
        match P::create(seed, &mut sock, scope) {
            None => return Err(Box::new(ProtocolStop)),
            Some((m, exp, dline)) => {
                let diff = dline - SteadyTime::now();
                let timeout = try!(scope.timeout_ms(
                    diff.num_milliseconds() as u64)
                    .map_err(|_| TimerError));
                Ok(Stream {
                    socket: sock,
                    expectation: exp,
                    deadline: dline,
                    timeout: timeout,
                    fsm: m,
                    inbuf: Buf::new(),
                    outbuf: Buf::new(),
                    phantom: PhantomData,
                })
            }
        }
    }
}

impl<C, S: StreamSocket, P: Protocol<C, S>> Machine<C> for Stream<C, S, P> {
    type Seed = Void;
    fn create(void: Void, _scope: &mut Scope<C>)
        -> Result<Self, Box<Error>>
    {
        unreachable(void);
    }
    fn ready(self, _events: EventSet, scope: &mut Scope<C>)
        -> Response<Self, Self::Seed>
    {
        // TODO(tailhook) use `events` to optimize reading
        let (fsm, exp, imp) = self.decompose();
        let deadline = imp.deadline;
        imp.action(Some((fsm, exp, deadline)), scope)
    }
    fn spawned(self, _scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        unreachable!();
    }
    fn timeout(self, scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        if Deadline::now() >= self.deadline {
            let (fsm, _exp, mut imp) = self.decompose();
            let res = fsm.timeout(&mut imp.transport(), scope);
            imp.action(res, scope)
        } else {
            // Spurious timeouts are possible for the couple of reasons
            Response::ok(self)
        }
    }
    fn wakeup(self, scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        let (fsm, _exp, mut imp) = self.decompose();
        let res = fsm.wakeup(&mut imp.transport(), scope);
        imp.action(res, scope)
    }
}

/// Protocol returned None right at the start of the stream processing
#[derive(Debug)]
pub struct ProtocolStop;

impl fmt::Display for ProtocolStop {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "ProtocolStop")
    }
}

impl Error for ProtocolStop {
    fn cause(&self) -> Option<&Error> { None }
    fn description(&self) -> &'static str {
        r#"Protocol returned None (which means "stop") at start"#
    }
}

/// Can't insert timer, so can't create a connection
///
/// It's may be because there are too many timers in mio event loop or
/// because timer is too far away in the future (this is a limitation of
/// mio event loop too)
#[derive(Debug)]
pub struct TimerError;

impl fmt::Display for TimerError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "error inserting timer")
    }
}

impl Error for TimerError {
    fn cause(&self) -> Option<&Error> { None }
    fn description(&self) -> &'static str {
        "Can't insert timer, probably too much timers or time is too far away"
    }
}

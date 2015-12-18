use std::fmt;
use std::error::Error;
use std::marker::PhantomData;

use time::SteadyTime;
use rotor::{Response, Scope, Machine};
use mio::{EventSet, PollOpt};
use netbuf::Buf;

use substr::find_substr;
use {Expectation, Protocol, StreamSocket, Stream, StreamImpl, Request};
use {Transport, Deadline};


impl<S: StreamSocket> StreamImpl<S> {
    fn _action<C, M>(mut self, mut req: Request<M>, scope: &mut Scope<C>)
        -> Result<Stream<C, S, M>, ()>
        where M: Protocol<C, S>,
              S: StreamSocket,
    {
        use Expectation::*;
        let mut req = try!(req.ok_or(()));
        let mut can_write = try!(self.write());
        'outer: loop {
            if can_write {
                can_write = try!(self.write());
            }
            match req.1 {
                Bytes(num) => {
                    loop {
                        if self.inbuf.len() >= num {
                            req = try!(req.0.bytes_read(&mut Transport {
                                inbuf: &mut self.inbuf,
                                outbuf: &mut self.outbuf,
                            }, num, scope).ok_or(()));
                            continue 'outer;
                        }
                        if !try!(self.read()) {
                            return Ok(Stream::compose(self, req, scope));
                        }
                    }
                }
                Delimiter(delim) => {
                    loop {
                        if let Some(num) = find_substr(&self.inbuf[..], delim) {
                            req = try!(req.0.bytes_read(&mut Transport {
                                inbuf: &mut self.inbuf,
                                outbuf: &mut self.outbuf,
                            }, num, scope).ok_or(()));
                            continue 'outer;
                        }
                        if !try!(self.read()) {
                            return Ok(Stream::compose(self, req, scope));
                        }
                    }
                }
                Flush(num) => {
                    if self.outbuf.len() < num {
                        req = try!(req.0.bytes_flushed(&mut Transport {
                            inbuf: &mut self.inbuf,
                            outbuf: &mut self.outbuf,
                        }, scope).ok_or(()));
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
        -> Response<Stream<C, S, M>, S>
        where M: Protocol<C, S>,
              S: StreamSocket
    {
        match self._action(req, scope) {
            Ok(x) => Response::ok(x),
            Err(()) => Response::done(),
        }
    }
    fn read(&mut self) -> Result<bool, ()> {
        unimplemented!();
    }
    fn write(&mut self) -> Result<bool, ()> {
        if self.outbuf.len() == 0 {
            return Ok(true);
        }
        unimplemented!();
    }
}

impl<C, S: StreamSocket, P: Protocol<C, S>> Stream<C, S, P> {
    fn decompose(self) -> (P, StreamImpl<S>) {
        (self.fsm, StreamImpl {
            socket: self.socket,
            expectation: self.expectation,
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
        Stream {
            fsm: fsm,
            socket: implem.socket,
            expectation: exp,
            deadline: dline, // TODO(tailhook) compare
            timeout: implem.timeout, // TODO(tailhook) compare
            inbuf: implem.inbuf,
            outbuf: implem.outbuf,
            phantom: PhantomData,
        }
    }
}

impl<C, S: StreamSocket, P: Protocol<C, S>> Machine<C> for Stream<C, S, P> {
    type Seed = S;
    fn create(mut sock: S, scope: &mut Scope<C>) -> Result<Self, Box<Error>> {
        // Always register everything in edge-triggered mode.
        // This allows to never reregister socket.
        //
        // The no-reregister strategy is not a goal (although, it's expected
        // to lower number of syscalls for many request-reply-like protocols)
        // but it allows to have single source of truth for
        // readable()/writable() mask (no duplication in kernel space)
        try!(scope.register(&sock,
            EventSet::readable() | EventSet::writable(), PollOpt::edge()));
        // TODO(tailhook) start
        match P::create(&mut sock, scope) {
            None => return Err(Box::new(ProtocolStop)),
            Some((m, exp, dline)) => {
                let diff = dline - SteadyTime::now();
                let timeout = try!(scope.timeout_ms(
                    diff.num_milliseconds() as u64).map_err(|_| TimerError));
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
    fn ready(self, events: EventSet, scope: &mut Scope<C>)
        -> Response<Self, Self::Seed>
    {
        unimplemented!();
    }
    fn spawned(self, scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        unreachable!();
    }
    fn timeout(self, scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        unimplemented!();
    }
    fn wakeup(self, scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        let (fsm, imp) = self.decompose();
        imp.action(fsm.wakeup(scope), scope)
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
        write!(fmt, "TimerError")
    }
}

impl Error for TimerError {
    fn cause(&self) -> Option<&Error> { None }
    fn description(&self) -> &'static str {
        "Can't insert timer, probably too much timers or time is too far away"
    }
}

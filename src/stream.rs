use rotor::{Response, Scope, Machine};
use mio::{EventSet, PollOpt};

use {Expectation, Protocol, StreamSocket, Stream, StreamImpl, Request};

macro_rules! rtry {
    ($x:expr) => {
        match $x {
            None => return Response::done(),
            Some(x) => x,
        }
    }
}


impl<S: StreamSocket> StreamImpl<S> {
    fn action<C, M>(r: Request<M>) -> Response<Stream<C, S, M>>
        where M: Protocol<C, S>,
              S: StreamSocket,
    {
        let (fsm, exp, deadline)  = rtry!(r);
        /*
        match fsm {
            Bytes(num) => {
                try_read(&mut socket)
            }
        }
        */
    }
}

impl<C, S: StreamSocket, P: Protocol<C, S>> Stream<C, S, P> {
    pub fn new(socket: S) -> Stream<C, S, P> {
        Stream {
            socket: socket,
            expectation: Expectation::Sleep,
            timeout: None,
            fsm: P::new(&mut socket),
        }
    }
    fn decompose(self) -> (P, StreamImpl<S>) {
        let (deadline, timeout) = self.timeout.expect("helolo");
        (self.fsm, StreamImpl {
            socket: self.socket,
            expectation: self.expectation,
            deadline: deadline,
            timeout: timeout,
        })
    }
}

impl<C, S: StreamSocket, P: Protocol<C, S>> Machine<C> for Stream<C, S, P> {
    fn register(mut self, scope: &mut Scope<C>) -> Response<Self> {
        // Always register everything in edge-triggered mode.
        // This allows to never reregister socket.
        //
        // The no-reregister strategy is not a goal (although, it's expected
        // to lower number of syscalls for many request-reply-like protocols)
        // but it allows to have single source of truth for
        // readable()/writable() mask (no duplication in kernel space)
        scope.register(&self.socket,
            EventSet::readable() | EventSet::writable(), PollOpt::edge())
            .unwrap();
        // TODO(tailhook) start
        Response::ok(self)
    }
    fn ready(self, events: EventSet, scope: &mut Scope<C>) -> Response<Self> {
    }
    fn spawned(self, scope: &mut Scope<C>) -> Response<Self> {
        unreachable!();
    }
    fn timeout(self, scope: &mut Scope<C>) -> Response<Self> {
        unimplemented!();
    }
    fn wakeup(self, scope: &mut Scope<C>) -> Response<Self> {
        let (fsm, imp) = self.decompose();
        imp.action(fsm.wakeup(scope))
    }
}

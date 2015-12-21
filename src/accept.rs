use std::error::Error;
use std::any::Any;

use rotor::{Machine, Scope, Response};
use mio::{TryAccept, EventSet, PollOpt, Evented};

use {StreamSocket, Accept};


pub trait Accepted<C, S: StreamSocket>: Machine<C> {
    fn accepted(sock: S, scope: &mut Scope<C>) -> Result<Self, Box<Error>>;
}


impl<A, S, M> Accept<A, M>
    where A: TryAccept<Output=S> + Evented + Any,
          S: StreamSocket,
{
    pub fn new<C>(sock: A, scope: &mut Scope<C>)
        -> Result<Self, Box<Error>>
    {
        try!(scope.register(&sock, EventSet::readable(), PollOpt::edge()));
        Ok(Accept::Server(sock))
    }
}

impl<C, A, S, M> Machine<C> for Accept<A, M>
    where A: TryAccept<Output=S> + Evented + Any,
          M: Machine<C> + Accepted<C, S>,
          S: StreamSocket,
{
    type Seed = S;
    fn create(sock: S, scope: &mut Scope<C>)
        -> Result<Self, Box<Error>>
    {
        M::accepted(sock, scope).map(Accept::Connection)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<C>)
        -> Response<Self, Self::Seed>
    {
        match self {
            Accept::Server(a) => {
                match a.accept() {
                    Ok(Some(sock)) => {
                        Response::spawn(Accept::Server(a), sock)
                    }
                    Ok(None) =>  {
                        Response::ok(Accept::Server(a))
                    }
                    Err(_) => {
                        // TODO(tailhook) maybe log the error
                        Response::ok(Accept::Server(a))
                    }
                }
            }
            Accept::Connection(m) => {
                m.ready(events, scope)
                    .map(Accept::Connection, |_| unreachable!())
            }
        }
    }

    fn spawned(self, _scope: &mut Scope<C>)
        -> Response<Self, Self::Seed>
    {
        match self {
            Accept::Server(a) => {
                match a.accept() {
                    Ok(Some(sock)) => {
                        Response::spawn(Accept::Server(a), sock)
                    }
                    Ok(None) =>  {
                        Response::ok(Accept::Server(a))
                    }
                    Err(_) => {
                        // TODO(tailhook) maybe log the error
                        Response::ok(Accept::Server(a))
                    }
                }
            }
            Accept::Connection(_) => {
                unreachable!();
            }
        }
    }

    fn timeout(self, scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        match self {
            Accept::Server(_) => unreachable!(),
            Accept::Connection(m) => {
                m.timeout(scope).map(Accept::Connection, |_| unreachable!())
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<C>) -> Response<Self, Self::Seed> {
        match self {
            Accept::Server(_) => unreachable!(),
            Accept::Connection(m) => {
                m.wakeup(scope).map(Accept::Connection, |_| unreachable!())
            }
        }
    }
}

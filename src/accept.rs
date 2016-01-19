use std::error::Error;
use std::any::Any;

use rotor::{Machine, Response, EventSet, PollOpt, Evented};
use rotor::{Scope, GenericScope};
use rotor::mio::{TryAccept};

use {StreamSocket, Accept};


pub trait Accepted<S: StreamSocket>: Machine {
    fn accepted(sock: S, scope: &mut Scope<Self::Context>)
        -> Result<Self, Box<Error>>;
}


impl<M, A> Accept<M, A>
    where A: TryAccept + Evented + Any,
          M: Machine
{
    pub fn new<S: GenericScope>(sock: A, scope: &mut S)
        -> Result<Self, Box<Error>>
    {
        try!(scope.register(&sock, EventSet::readable(), PollOpt::edge()));
        Ok(Accept::Server(sock))
    }
}

impl<M, A, S> Machine for Accept<M, A>
    where A: TryAccept<Output=S> + Evented + Any,
          S: StreamSocket,
          M: Machine + Accepted<S>,
{
    type Context = M::Context;
    type Seed = S;
    fn create(sock: S, scope: &mut Scope<Self::Context>)
        -> Result<Self, Box<Error>>
    {
        M::accepted(sock, scope).map(Accept::Connection)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<Self::Context>)
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

    fn spawned(self, _scope: &mut Scope<Self::Context>)
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

    fn timeout(self, scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        match self {
            Accept::Server(_) => unreachable!(),
            Accept::Connection(m) => {
                m.timeout(scope).map(Accept::Connection, |_| unreachable!())
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        match self {
            Accept::Server(_) => unreachable!(),
            Accept::Connection(m) => {
                m.wakeup(scope).map(Accept::Connection, |_| unreachable!())
            }
        }
    }
}

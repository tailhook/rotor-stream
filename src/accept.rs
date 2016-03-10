use std::any::Any;

use rotor::{Machine, Response, EventSet, PollOpt, Evented};
use rotor::{Scope, GenericScope, Void};
use rotor::mio::{TryAccept};

use {StreamSocket, Accept};


/// Trait which must be implemented for a state machine to accept connection
///
/// This basically provides alternative constructor for the state machine.
pub trait Accepted: Machine {
    type Seed: Clone;
    type Socket: StreamSocket;
    /// The constructor of the state machine from the accepted connection
    fn accepted(sock: Self::Socket, seed: <Self as Accepted>::Seed,
        scope: &mut Scope<Self::Context>)
        -> Response<Self, Void>;
}


impl<M, A> Accept<M, A>
    where A: TryAccept<Output=M::Socket> + Evented + Any,
          M: Accepted,
{
    pub fn new<S: GenericScope>(sock: A,
        seed: <M as Accepted>::Seed, scope: &mut S)
        -> Response<Self, Void>
    {
        match scope.register(&sock, EventSet::readable(), PollOpt::edge()) {
            Ok(()) => {}
            Err(e) => return Response::error(Box::new(e)),
        }
        Response::ok(Accept::Server(sock, seed))
    }
}

impl<M, A> Machine for Accept<M, A>
    where A: TryAccept<Output=M::Socket> + Evented + Any,
          M: Accepted,
{
    type Context = M::Context;
    type Seed = (A::Output, <M as Accepted>::Seed);
    fn create((sock, seed): Self::Seed, scope: &mut Scope<Self::Context>)
        -> Response<Self, Void>
    {
        M::accepted(sock, seed, scope).wrap(Accept::Connection)
    }

    fn ready(self, events: EventSet, scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        match self {
            Accept::Server(a, s) => {
                match a.accept() {
                    Ok(Some(sock)) => {
                        let seed = (sock, s.clone());
                        Response::spawn(Accept::Server(a, s), seed)
                    }
                    Ok(None) =>  {
                        Response::ok(Accept::Server(a, s))
                    }
                    Err(_) => {
                        // TODO(tailhook) maybe log the error
                        Response::ok(Accept::Server(a, s))
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
            Accept::Server(a, s) => {
                match a.accept() {
                    Ok(Some(sock)) => {
                        let seed = (sock, s.clone());
                        Response::spawn(Accept::Server(a, s), seed)
                    }
                    Ok(None) =>  {
                        Response::ok(Accept::Server(a, s))
                    }
                    Err(_) => {
                        // TODO(tailhook) maybe log the error
                        Response::ok(Accept::Server(a, s))
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
            Accept::Server(..) => unreachable!(),
            Accept::Connection(m) => {
                m.timeout(scope).map(Accept::Connection, |_| unreachable!())
            }
        }
    }

    fn wakeup(self, scope: &mut Scope<Self::Context>)
        -> Response<Self, Self::Seed>
    {
        match self {
            me @ Accept::Server(..) => Response::ok(me),
            Accept::Connection(m) => {
                m.wakeup(scope).map(Accept::Connection, |_| unreachable!())
            }
        }
    }
}

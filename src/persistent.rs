use std::fmt::Debug;
use std::error::Error;

use rotor::{Machine, EventSet, PollOpt, Scope, Response, Time};
use rotor::void::{unreachable, Void};
use rotor::{GenericScope};

use {ActiveStream, Protocol, Stream, ProtocolStop, Transport};
use extensions::{ResponseExt, ScopeExt};


/// Reconnect timeout in milliseconds.
///
/// TODO(tailhook) should be overridable at runtime
pub const RECONNECT_TIMEOUT: u64 = 200;

/// Time for establishing a connection
///
/// TODO(tailhook) should be overridable at runtime
pub const CONNECT_TIMEOUT: u64 = 1_000;


/// Persistent client connection
///
/// TODO(tailhook) this should include name resolution
pub struct Persistent<P>(<P::Socket as ActiveStream>::Address,
                         P::Seed, Fsm<P>)
    where P: Protocol, P::Socket: ActiveStream;

#[derive(Debug)]
pub enum Fsm<P: Protocol> {
    Idle,
    Connecting(P::Socket, Time),
    Established(Stream<P>),
    Sleeping(Time),
}

fn response<P>(addr: <P::Socket as ActiveStream>::Address,
    seed: P::Seed, fsm: Fsm<P>)
    -> Response<Persistent<P>, Void>
    where P: Protocol, P::Socket: ActiveStream
{
    use self::Fsm::*;
    let timeo = match *&fsm {
        Idle => None,
        Connecting(_, tm) => Some(tm),
        // Can't find out a timeout for established connection
        // some other way should be used for this case
        Established(..) => unreachable!(),
        Sleeping(tm) => Some(tm),
    };
    Response::ok(Persistent(addr, seed, fsm))
        .deadline_opt(timeo)
}

impl<P> Persistent<P>
    where P: Protocol,
          P::Socket: ActiveStream,
          <P::Socket as ActiveStream>::Address: Debug
{
    pub fn new<S: GenericScope>(_scope: &mut S,
            address: <P::Socket as ActiveStream>::Address, seed: P::Seed)
        -> Response<Persistent<P>, Void>
    {
        Response::ok(Persistent(address, seed, Fsm::Idle))
    }

    pub fn connect<S: GenericScope>(scope: &mut S,
            address: <P::Socket as ActiveStream>::Address, seed: P::Seed)
        -> Response<Persistent<P>, Void>
    {
        let fsm = match P::Socket::connect(&address) {
            Ok(sock) => {
                scope.register(&sock, EventSet::writable(), PollOpt::level())
                    .expect("Can't register socket");
                Fsm::Connecting(sock, scope.after(CONNECT_TIMEOUT))
            }
            Err(e) => {
                info!("Failed to connect to {:?}: {}", address, e);
                Fsm::Sleeping(scope.after(RECONNECT_TIMEOUT))
            }
        };
        response(address, seed, fsm)
    }
}

impl<P> Persistent<P>
    where P: Protocol, P::Socket: ActiveStream
{
    /// Get a `Transport` object of the underlying stream
    ///
    /// This method is only useful if you want to manipulate buffers
    /// externally (like pushing to the buffer from another thread). Just be
    /// sure to **wake up** state machine after manipulating buffers.
    ///
    /// Returns `None` if stream is not currently connected
    pub fn transport(&mut self) -> Option<Transport<P::Socket>> {
        match self.2 {
            Fsm::Established(ref mut s) => Some(s.transport()),
            _ => None,
        }
    }
}

impl<P> Fsm<P>
    where P: Protocol,
          P::Seed: Clone,
          P::Socket: ActiveStream,
          <P::Socket as ActiveStream>::Address: Debug
{
    fn action<S: GenericScope>(resp: Response<Stream<P>, Void>,
        addr: <P::Socket as ActiveStream>::Address,
        seed: P::Seed, scope: &mut S)
        -> Response<Persistent<P>, Void>
    {
        if resp.is_stopped() {
            if let Some(err) = resp.cause() {
                warn!("Connection is failed: {}", err);
            } else {
                warn!("Connection is stopped by protocol");
            }
            response(addr, seed,
                Fsm::Sleeping(scope.after(RECONNECT_TIMEOUT)))
        } else {
            resp
                .wrap(Fsm::Established)
                .wrap(|x| Persistent(addr, seed, x))
        }
    }
}

impl<P: Protocol> Machine for Persistent<P>
    where P: Protocol,
          P::Seed: Clone,
          P::Socket: ActiveStream,
          <P::Socket as ActiveStream>::Address: Debug
{
    type Context = P::Context;
    type Seed = Void;
    fn create(seed: Self::Seed, _scope: &mut Scope<P::Context>)
        -> Response<Self, Void>
    {
        unreachable(seed)
    }
    fn ready(self, events: EventSet, scope: &mut Scope<P::Context>)
        -> Response<Self, Self::Seed>
    {
        use self::Fsm::*;
        let Persistent(addr, seed, state) = self;
        let state = match state {
            Idle => Idle,  // spurious event
            Connecting(sock, dline) => {
                if events.is_writable() {
                    let resp =  Stream::connected(sock, seed.clone(), scope);
                    if resp.is_stopped() {
                        error!("Error creating stream FSM: {}",
                            resp.cause().unwrap_or(&ProtocolStop));
                        Fsm::Sleeping(scope.after(RECONNECT_TIMEOUT))
                    } else {
                        return resp
                            .wrap(Established)
                            .wrap(|x| Persistent(addr, seed, x))
                    }
                } else if events.is_hup() {
                    error!("Connection closed immediately");
                    Fsm::Sleeping(scope.after(RECONNECT_TIMEOUT))
                } else {
                    Connecting(sock, dline) // spurious event
                }
            }
            Established(x) => {
                return Fsm::action(x.ready(events, scope), addr, seed, scope);
            }
            Sleeping(dline) => Sleeping(dline), // spurious event
        };
        response(addr, seed, state)
    }
    fn spawned(self, _scope: &mut Scope<P::Context>)
        -> Response<Self, Self::Seed>
    {
        unreachable!();
    }
    fn timeout(self, scope: &mut Scope<P::Context>)
        -> Response<Self, Self::Seed>
    {
        use self::Fsm::*;
        let Persistent(addr, seed, state) = self;
        let state = match state {
            Idle => Idle,  // spurious timeout
            Connecting(sock, dline) => {
                if scope.now() >= dline {
                    warn!("Timeout while establishing connection");
                    Fsm::Sleeping(scope.after(RECONNECT_TIMEOUT))
                } else {  // spurious timeout
                    Connecting(sock, dline)
                }
            }
            Established(x) => {
                return Fsm::action(x.timeout(scope), addr, seed, scope);
            }
            Sleeping(dline) => {
                if scope.now() >= dline {
                    return Self::connect(scope, addr, seed);
                } else {
                    Sleeping(dline)  // spurious timeout
                }
            }
        };
        response(addr, seed, state)
    }
    fn wakeup(self, scope: &mut Scope<P::Context>)
        -> Response<Self, Self::Seed>
    {
        use self::Fsm::*;
        let Persistent(addr, seed, state) = self;
        let state = match state {
            Established(x) => {
                return Fsm::action(x.wakeup(scope), addr, seed, scope);
            }
            x => x, // spurious wakeup
        };
        response(addr, seed, state)
    }
}

#[cfg(feature="replaceable")]
mod replaceable {

    use std::fmt::Debug;

    use {ActiveStream, Protocol, Persistent};
    use rotor_tools::sync::Replaceable;

    use super::Fsm;

    impl<P: Protocol> Replaceable for Persistent<P>
        where P: Protocol,
              P::Seed: Clone,
              <P::Socket as ActiveStream>::Address: Clone + Debug,
              P::Socket: ActiveStream
    {
        fn empty(&self) -> Self {
            // We assume that cloning is cheap enough. Probably just Copy
            Persistent(self.0.clone(), self.1.clone(), Fsm::Idle)
        }
    }
}

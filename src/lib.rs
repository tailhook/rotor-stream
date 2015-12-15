extern crate netbuf;
extern crate memchr;
extern crate rotor;
extern crate time;
extern crate mio;

mod substr;
mod transport;
mod protocol;

pub use protocol::{Protocol, Expectation};


pub struct Transport<'a> {
    inbuf: &'a mut netbuf::Buf,
    outbuf: &'a mut netbuf::Buf,
}

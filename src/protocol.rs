use std::io;
use rotor::Scope;

use {Transport, Intent, StreamSocket};

quick_error!{
    #[derive(Debug)]
    pub enum Exception {
        /// End of stream reached (when reading)
        ///
        /// This may be not a broken expectation, we just notify of end of
        /// stream always (if the state machine is still alive)
        ///
        /// Note: the equivalent of end of stream for write system call is
        /// translated to `WriteError(WriteZero)`
        EndOfStream {
            description("end of stream reached")
        }
        /// Limit for the number of bytes reached
        ///
        /// This is called when there is alredy maximum bytes in the buffer
        /// (third argument of `Delimiter`) but no delimiter found.
        LimitReached {
            description("reached the limit of bytes buffered")
        }
        ReadError(err: io::Error) {
            description("error when reading from stream")
            display("read error: {}", err)
        }
        WriteError(err: io::Error) {
            description("error when writing to stream")
            display("write error: {}", err)
        }
    }
}


// #[derive(Clone, Clone)]
// This could be Copy, but I think it could be implemented efficient enough
// without Copy and Clone. Probably we will enable them for the user code later
#[derive(Debug)]
pub enum Expectation {
    /// Read number of bytes
    ///
    /// The buffer that is passed to bytes_read might contain more bytes, but
    /// `num` parameter of the `bytes_read()` method will contain a number of
    /// bytes passed into `Bytes` constructor.
    ///
    /// Note that bytes passed here is neither limit on bytes actually read
    /// from the network (we resize buffer as convenient for memory allocator
    /// and read as much as possible), nor is the preallocated buffer size
    /// (we don't preallocate the buffer to be less vulnerable to DoS attacks).
    ///
    /// Note that real number of bytes that `netbuf::Buf` might contain is less
    /// than 4Gb. So this value can't be as big as `usize::MAX`
    Bytes(usize),
    /// Read until delimiter
    ///
    /// Parameters: `offset`, `delimiter`, `max_bytes`
    ///
    /// Only static strings are supported for delimiter now.
    ///
    /// `bytes_read` action gets passed `num` bytes before the delimeter, or
    /// in other words, the position of the delimiter in the buffer.
    /// The delimiter is guaranteed to be in the buffer too. The `max_bytes`
    /// do include the offset itself.
    ///
    Delimiter(usize, &'static [u8], usize),
    /// Wait until no more than N bytes is in output buffer
    ///
    /// This is going to be used for several cases:
    ///
    /// 1. `Flush(0)` before closing the connection
    /// 2. `Flush(0)` to before receiving new request (if needed)
    /// 3. `Flush(N)` to wait when you can continue producing some data, this
    ///    allows TCP pushback. To be able not to put everything in output
    ///    buffer at once. Still probably more efficient than `Flush(0)`
    Flush(usize),
    /// Wait until deadline
    ///
    /// This useful for two cases:
    ///
    /// 1. Just wait before doing anything if required by business logic
    /// 2. Wait until `wakeup` happens or atimeout whatever comes first
    Sleep,
}

pub trait Protocol: Sized {
    type Context;
    type Socket: StreamSocket;
    type Seed;
    /// Starting the protocol (e.g. accepted a socket)
    // TODO(tailhook) transport be here instead of sock?
    fn create(seed: Self::Seed, sock: &mut Self::Socket,
        scope: &mut Scope<Self::Context>)
        -> Intent<Self>;

    /// The action WaitBytes or WaitDelimiter is complete
    ///
    /// Note you don't have to consume input buffer. The data is in the
    /// transport, but you are free to ignore it. This may be useful for
    /// example to yield `Bytes(4)` to read the header size and then yield
    /// bigger value to read the whole header at once. But be careful, if
    /// you don't consume bytes you will repeatedly receive them again.
    fn bytes_read(self, transport: &mut Transport<Self::Socket>,
                  end: usize, scope: &mut Scope<Self::Context>)
        -> Intent<Self>;

    /// The action Flush is complete
    fn bytes_flushed(self, transport: &mut Transport<Self::Socket>,
                     scope: &mut Scope<Self::Context>)
        -> Intent<Self>;

    /// Timeout happened, which means either deadline reached in
    /// Bytes, Delimiter, Flush. Or Sleep has passed.
    fn timeout(self, transport: &mut Transport<Self::Socket>,
        scope: &mut Scope<Self::Context>)
        -> Intent<Self>;

    /// The method is called when too much bytes are read but no delimiter
    /// is found within the number of bytes specified. Or end of stream reached
    ///
    /// The usual case is to just close the connection (because it's probably
    /// DoS attack is going on or the protocol mismatch), but sometimes you
    /// want to send error code, like 413 Entity Too Large for HTTP.
    ///
    /// Note it's your responsibility to wait for the buffer to be flushed.
    /// If you write to the buffer and then return Intent::done() immediately,
    /// your data will be silently discarded.
    fn exception(self, _transport: &mut Transport<Self::Socket>,
        reason: Exception, _scope: &mut Scope<Self::Context>)
        -> Intent<Self>
    {
        Intent::error(Box::new(reason))
    }

    /// Message received (from the main loop)
    fn wakeup(self, transport: &mut Transport<Self::Socket>,
        scope: &mut Scope<Self::Context>)
        -> Intent<Self>;
}

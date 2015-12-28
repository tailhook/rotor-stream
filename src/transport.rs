use netbuf::Buf;
use {Transport, StreamSocket};


/// Transport is thing that provides buffered I/O for stream sockets
impl<'a, S: StreamSocket> Transport<'a, S> {
    /// Get the reference to the underlying stream
    ///
    /// It's here so you can inspect socket state or tune it. For example you
    /// might want to set TCP_CORK option or find out peer or local address.
    ///
    /// Reading from and writing to a socket directly may lead to unexpected
    /// behavior. Use `input()` and `output()` buffers instead.
    pub fn socket<'x>(&'x mut self) -> &'x mut S {
        self.sock
    }
    /// Get a reference to the input buffer
    ///
    /// It's expected that you only read and `consume()` bytes from buffer
    /// but never write.
    pub fn input<'x>(&'x mut self) -> &'x mut Buf {
        self.inbuf
    }
    /// Get a reference to the output buffer
    ///
    /// It's expected that you only inspect and write to the output buffer
    /// but never `consume()`
    pub fn output<'x>(&'x mut self) -> &'x mut Buf {
        self.outbuf
    }
    /// Get a references to both buffers (input, output)
    ///
    /// It's useful when you want to pass both things somewhere along the
    /// chain of calls. See `input()` and `output()` methods for more comments
    /// on buffer usage
    pub fn buffers<'x>(&'x mut self) -> (&'x mut Buf, &'x mut Buf) {
        (self.inbuf, self.outbuf)
    }
}

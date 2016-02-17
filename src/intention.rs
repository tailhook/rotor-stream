use std::error::Error;

use rotor::Time;


use {Expectation, Intent, IntentBuilder};

impl<M> Intent<M> {
    /// Start building the Intent object of the state machine
    pub fn of(machine: M) -> IntentBuilder<M> {
        IntentBuilder(machine)
    }
    /// Notifies that state machine has done it's work
    pub fn done() -> Self {
        Intent(Err(None), Expectation::Sleep, None)
    }
    /// Notifies that we have protocol error and connection should be closed
    pub fn error(e: Box<Error>) -> Self {
        Intent(Err(Some(e)), Expectation::Sleep, None)
    }
    /// Add/change the deadline
    ///
    /// Note: if you skip this method on next return timeout will be reset.
    /// Which means this state machine may hang indefinitely.
    pub fn deadline(self, deadline: Time) -> Intent<M> {
        Intent(self.0, self.1, Some(deadline))
    }
    /// Add/change the deadline as an optional value
    ///
    /// Note this will reset timeout if deadline is None, not keep unchanged
    pub fn deadline_opt(self, deadline: Option<Time>) -> Intent<M> {
        Intent(self.0, self.1, deadline)
    }
}

impl<M> IntentBuilder<M> {
    pub fn expect_bytes(self, min_bytes: usize) -> Intent<M> {
        Intent(Ok(self.0), Expectation::Bytes(min_bytes), None)
    }
    pub fn expect_delimiter(self, delim: &'static [u8], max_bytes: usize)
        -> Intent<M>
    {
        Intent(Ok(self.0),
               Expectation::Delimiter(0, delim, max_bytes), None)
    }
    pub fn expect_delimiter_after(self, offset: usize,
        delim: &'static [u8], max_bytes: usize)
        -> Intent<M>
    {
        Intent(Ok(self.0),
               Expectation::Delimiter(offset, delim, max_bytes), None)
    }
    pub fn expect_flush(self) -> Intent<M> {
        Intent(Ok(self.0), Expectation::Flush(0), None)
    }
    pub fn sleep(self) -> Intent<M> {
        Intent(Ok(self.0), Expectation::Sleep, None)
    }
}

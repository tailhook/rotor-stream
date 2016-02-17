use std::fmt;
use std::error::Error;

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

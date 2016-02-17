use std::time::Duration;

use rotor::{Time, Response, GenericScope};


pub trait ResponseExt {
    fn deadline_opt(self, deadline: Option<Time>) -> Self;
}

impl<M, N> ResponseExt for Response<M, N> {
    fn deadline_opt(self, deadline: Option<Time>) -> Self {
        if let Some(time) = deadline {
            self.deadline(time)
        } else {
            self
        }
    }
}

pub trait ScopeExt {
    fn reached(&self, deadline: Option<Time>) -> bool;
    fn after(&self, milliseconds: u64) -> Time;
}

impl<T: GenericScope> ScopeExt for T {
    fn reached(&self, deadline: Option<Time>) -> bool {
        deadline.map(|x| self.now() >= x).unwrap_or(false)
    }
    fn after(&self, milliseconds: u64) -> Time {
        self.now() + Duration::from_millis(milliseconds)
    }
}

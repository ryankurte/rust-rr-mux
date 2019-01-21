
#[macro_use]
extern crate log;


pub mod connector;
pub use crate::connector::Connector;

pub mod mux;
pub use crate::mux::{Mux, Muxed};

pub mod mapped;

pub mod mock;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

#[macro_use]
extern crate log;

pub mod connector;
/// Connector defines a generic futures-based request/response interface.
/// This can be used to implement message based protocols independent of underlying transports
pub use crate::connector::Connector;

pub mod muxed;
/// Muxed describes a message that is either a Request or a Response
pub use muxed::Muxed;

pub mod mux;
/// Mux is an implementation of a Connector using a HashMap and oneshot channels
pub use crate::mux::Mux;

pub mod mapped;
/// Mapped converts a connector interface from one type to another using a Mapper implementation.
/// This can be used to multiplex protocols / message types over a single base connector
pub use mapped::{Mapped, Mapper};

/// Mock is a mock connector implementation that allows expectation based testing of modules that consume
/// the Connector interface
pub mod mock;

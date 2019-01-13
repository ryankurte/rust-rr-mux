
use futures::Future;

/// Connector provides support for making and responding to requests
pub trait Connector<ID, ADDR, REQ, RESP, ERR> {
    // Send a request and receive a response or error at some time in the future
    fn request(&mut self, id: ID, addr: ADDR, req: REQ) -> Box<Future<Item=RESP, Error=ERR>>;

    // Send a response message
    fn respond(&mut self, id: ID, addr: ADDR, resp: RESP) -> Box<Future<Item=(), Error=ERR>>;
}

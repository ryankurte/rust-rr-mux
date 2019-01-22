
use futures::Future;

/// Connector provides generic support for making and responding to requests
/// This allows protocols to be implemented over an arbitrary transport
pub trait Connector<REQ_ID, TARGET, REQ, RESP, ERR, CTX> {
    // Send a request and receive a response or error at some time in the future
    fn request(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, req: REQ) -> Box<Future<Item=RESP, Error=ERR> + Send + 'static>;

    // Send a response message
    fn respond(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, resp: RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static>;
}



use futures::Future;

/// Requester is a futures based request/response function
pub type Requester<ID, TARGET, REQ, RESP, ERR, CTX> = FnMut(CTX, ID, TARGET, REQ) -> Box<Future<Item=RESP, Error=ERR> + Send + 'static>;

/// Connector provides support for making and responding to requests
pub trait Connector<REQ_ID, TARGET, REQ, RESP, ERR, CTX> {
    // Send a request and receive a response or error at some time in the future
    fn request(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, req: REQ) -> Box<Future<Item=RESP, Error=ERR> + Send + 'static>;

    // Send a response message
    fn respond(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, resp: RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static>;
}


use futures::Future;

/// Connector provides generic support for making and responding to requests
/// This allows protocols to be implemented over an arbitrary transport
/// ReqId is an ID used to associate requests and responses
/// Target is a target address
/// REQ is a request type
/// RESP is a response type
/// E is the underlying connector error
/// Ctx is a context passed through requests to underlying handlers
pub trait Connector<ReqId, Target, Req, Resp, E, Ctx> {
    // Send a request and receive a response or error at some time in the future
    fn request(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, req: Req,
    ) -> Box<Future<Item = Resp, Error = E> + Send + 'static>;

    // Send a response message
    fn respond(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, resp: Resp,
    ) -> Box<Future<Item = (), Error = E> + Send + 'static>;
}

use async_trait::async_trait;

/// Connector provides generic support for making and responding to requests
/// This allows protocols to be implemented over an arbitrary transport
/// ReqId is an ID used to associate requests and responses
/// Target is a target address
/// REQ is a request type
/// RESP is a response type
/// E is the underlying connector error
/// Ctx is a context passed through requests to underlying handlers
#[async_trait]
pub trait Connector<ReqId, Target, Req, Resp, E, Ctx> {
    // Send a request and receive a response or error at some time in the future
    async fn request(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, req: Req,
    ) -> Result<Resp, E>;

    // Send a response message
    async fn respond(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, resp: Resp,
    ) -> Result<(), E>;
}

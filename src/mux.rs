use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use futures::prelude::*;
use futures::sync::mpsc::{channel, Receiver as ChannelReceiver, Sender as ChannelSender};
use futures::sync::{oneshot, oneshot::Sender as OneshotSender};

use crate::connector::Connector;
use crate::muxed::Muxed;

/// Mux is a futures based request response multiplexer.
/// This provides a Source interface to drain messages sent, and receives messages via the handle() method,
/// allowing responses to be consumed and requests forwarded on.
///
/// ReqId is the request ReqId type
/// Target is the target for the Req or Resp to be sent to
/// Req and Resp are the request and response messages
/// Ctx is a a shared context
pub struct Mux<ReqId, Target, Req, Resp, E, Ctx> {
    requests: Arc<Mutex<HashMap<ReqId, Box<OneshotSender<(Resp, Ctx)>>>>>,

    sender: ChannelSender<(ReqId, Target, Muxed<Req, Resp>, Ctx)>,
    receiver: Arc<Mutex<ChannelReceiver<(ReqId, Target, Muxed<Req, Resp>, Ctx)>>>,

    _addr: PhantomData<Target>,
    _req: PhantomData<Req>,
    _err: PhantomData<E>,
    _ctx: PhantomData<Ctx>,
}

impl<ReqId, Target, Req, Resp, E, Ctx> Clone for Mux<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + Sync + Send + 'static,
    Target: Debug + Sync + Send + 'static,
    Req: Debug + Sync + Send + 'static,
    Resp: Debug + Sync + Send + 'static,
    E: Debug + Sync + Send + 'static,
    Ctx: Debug + Clone + Sync + Send + 'static,
{
    fn clone(&self) -> Self {
        Mux {
            requests: self.requests.clone(),
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
            _ctx: PhantomData,
            _addr: PhantomData,
            _req: PhantomData,
            _err: PhantomData,
        }
    }
}

impl<ReqId, Target, Req, Resp, E, Ctx> Mux<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + Sync + Send + 'static,
    Target: Debug + Sync + Send + 'static,
    Req: Debug + Sync + Send + 'static,
    Resp: Debug + Sync + Send + 'static,
    E: Debug + Sync + Send + 'static,
    Ctx: Debug + Clone + Sync + Send + 'static,
{
    /// Create a new mux over the provided sender
    pub fn new() -> Mux<ReqId, Target, Req, Resp, E, Ctx> {
        let (tx, rx) = channel(0);

        Mux {
            requests: Arc::new(Mutex::new(HashMap::new())),
            sender: tx,
            receiver: Arc::new(Mutex::new(rx)),
            _ctx: PhantomData,
            _addr: PhantomData,
            _req: PhantomData,
            _err: PhantomData,
        }
    }

    /// Handle a muxed received message
    /// This either returns a pending response or passes request messages on
    pub fn handle(
        &mut self, id: ReqId, addr: Target, message: Muxed<Req, Resp>, ctx: Ctx) -> Result<Option<(Target, Req, Ctx)>, E> {
        let r = match message {
            // Requests get passed through the mux
            Muxed::Request(req) => Some((addr, req, ctx)),
            // Responses get matched with outstanding requests
            Muxed::Response(resp) => {
                self.handle_resp(id, addr, resp, ctx)?;
                None
            }
        };

        Ok(r)
    }

    /// Handle a pre-decoded response message
    pub fn handle_resp(&mut self, id: ReqId, _target: Target, resp: Resp, ctx: Ctx) -> Result<(), E> {
        if let Some(ch) = self.requests.lock().unwrap().remove(&id) {
            ch.send((resp, ctx)).unwrap();
        } else {
            info!("Response id: '{:?}', no request pending", id);
        }
        Ok(())
    }
}

impl<ReqId, Target, Req, Resp, E, Ctx> Connector<ReqId, Target, Req, Resp, E, Ctx>
    for Mux<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: std::cmp::Eq + std::hash::Hash + Debug + Clone + Send + 'static,
    Target: Debug + Send + 'static,
    Req: Debug + Send + 'static,
    Resp: Debug + Send + 'static,
    E: Debug + Send + 'static,
    Ctx: Debug + Clone + Send + 'static,
{
    /// Send and register a request
    fn request(
        &mut self, ctx: Ctx, id: ReqId, addr: Target, req: Req,
    ) -> Box<Future<Item = (Resp, Ctx), Error = E> + Send + 'static> {
        // Create future channel
        let (tx, rx) = oneshot::channel();

        // Save response to map
        self.requests
            .lock()
            .unwrap()
            .insert(id.clone(), Box::new(tx));

        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(
            futures::lazy(move || {
                sender
                    .send((id, addr, Muxed::Request(req), ctx))
                    .map(|_r| ())
                    .map_err(|_e| panic!())
            })
            .and_then(|_| {
                // Panic on future closed, this is probably not desirable
                // TODO: fix this
                rx.map_err(|_e| panic!())
            }),
        )
    }

    fn respond(
        &mut self, ctx: Ctx, id: ReqId, addr: Target, resp: Resp,
    ) -> Box<Future<Item = (), Error = E> + Send + 'static> {
        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            sender
                .send((id, addr, Muxed::Response(resp), ctx))
                .map(|_r| ())
                .map_err(|_e| panic!())
        }))
    }
}

impl<ReqId, Target, Req, Resp, E, Ctx> Stream for Mux<ReqId, Target, Req, Resp, E, Ctx> {
    type Item = (ReqId, Target, Muxed<Req, Resp>, Ctx);
    type Error = ();

    // Poll to read pending requests
    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        self.receiver.lock().unwrap().poll()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[derive(PartialEq, Debug, Clone)]
    struct A(u64);
    #[derive(PartialEq, Debug, Clone)]
    struct B(u64);
    #[derive(PartialEq, Debug, Clone)]
    struct C(u64);

    #[test]
    fn text_mux() {
        let mut mux: Mux<u16, u32, A, B, (), C> = Mux::new();

        let req_id = 10;
        let addr = 12;
        let req = A(20);
        let resp = B(30);

        let ctx_out = C(40);
        let ctx_in = C(50);

        let r = resp.clone();
        let c = ctx_in.clone();

        // Make a request and check the response
        let a = mux.clone().request(ctx_out.clone(), req_id.clone(), addr.clone(), req.clone())
            .map(|(resp, ctx)| {
                assert_eq!(r, resp);
                assert_eq!(c, ctx);
            }).map_err(|_e| () );


        // Respond to request
        let b = mux.clone().for_each(|(i, a, m, c)| {
            let req_id = req_id.clone();
            assert_eq!(i, req_id);
            assert_eq!(a, addr);
            assert_eq!(m.req().unwrap(), req);
            assert_eq!(c, ctx_out);

            let resp = resp.clone();
            let ctx_in = ctx_in.clone();
            
            mux.handle_resp(req_id, addr, resp, ctx_in)
        }).map(|_| () ).map_err(|_e| () );

        // Run using select
        // a will finish, b will poll forever
        let _ = Future::select(a, b).wait();

    }
}
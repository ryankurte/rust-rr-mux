use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::pin::Pin;

use futures::prelude::*;
use futures::stream::Stream;
use futures::channel::mpsc::{channel, Receiver as ChannelReceiver, Sender as ChannelSender};
use futures::channel::{oneshot, oneshot::Sender as OneshotSender};
use futures::task::{Context, Poll};
use async_trait::async_trait;

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
        let ch = { self.requests.lock().unwrap().remove(&id) };
        if let Some(ch) = ch {
            ch.send((resp, ctx)).unwrap();
        } else {
            info!("Response id: '{:?}', no request pending", id);
        }
        Ok(())
    }
}

#[async_trait]
impl<ReqId, Target, Req, Resp, E, Ctx> Connector<ReqId, Target, Req, Resp, E, Ctx>
    for Mux<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: std::cmp::Eq + std::hash::Hash + Debug + Clone + Send + 'static,
    Target: Debug + Sync + Send + 'static,
    Req: Debug + Send + 'static,
    Resp: Debug + Send + 'static,
    E: Debug + Send + 'static,
    Ctx: Debug + Clone + Send + 'static,
{
    /// Send and register a request
    async fn request(
        &mut self, ctx: Ctx, id: ReqId, addr: Target, req: Req,
    ) -> Result<(Resp, Ctx), E> {
        // Create future channel
        let (tx, rx) = oneshot::channel();

        // Save response to map
        { self.requests
            .lock()
            .unwrap()
            .insert(id.clone(), Box::new(tx)) };

        // Send request and return channel future
        let mut sender = self.sender.clone();

        match sender.send((id, addr, Muxed::Request(req), ctx)).await {
            Ok(_) => (),
            Err(e) => panic!(e),
        };

        let res = match rx.await {
            Ok(r) => r,
            Err(e) => panic!(e),
        };

        Ok(res)
    }

    async fn respond(
        &mut self, ctx: Ctx, id: ReqId, addr: Target, resp: Resp,
    ) -> Result<(), E> {
        // Send request and return channel future
        let mut sender = self.sender.clone();

        match sender.send((id, addr, Muxed::Response(resp), ctx)).await {
            Ok(_) => (),
            Err(e) => panic!(e),
        };

        Ok(())
    }
}

// Stream implementation to allow polling from mux
impl<ReqId, Target, Req, Resp, E, Ctx> Stream for Mux<ReqId, Target, Req, Resp, E, Ctx> {
    type Item = (ReqId, Target, Muxed<Req, Resp>, Ctx);

    // Poll to read pending requests
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.receiver.lock().unwrap().poll_next_unpin(cx)
    }
}


#[cfg(test)]
mod tests {
    extern crate futures;
    use futures::prelude::*;
    use futures::executor::block_on;

    use super::*;

    #[derive(PartialEq, Debug, Copy, Clone)]
    struct A(u64);
    #[derive(PartialEq, Debug, Copy, Clone)]
    struct B(u64);
    #[derive(PartialEq, Debug, Copy, Clone)]
    struct C(u64);

    #[test]
    fn test_mux() {
        let mut mux: Mux<u16, u32, A, B, (), C> = Mux::new();

        let req_id = 10;
        let addr = 12;
        let req = A(20);
        let resp = B(30);

        let ctx_out = C(40);
        let ctx_in = C(50);

        // Make a request and check the response
        let mut m = mux.clone();
        let a = async {
            let (r, c) = m.request(ctx_out, req_id, addr, req).await.unwrap();
            assert_eq!(resp, r);
            assert_eq!(ctx_in, c);
        }.boxed();

        // Respond to request
        let b = async {
            while let Some((i, a, m, c)) = mux.next().await {
                assert_eq!(i, req_id);
                assert_eq!(a, addr);
                assert_eq!(m.req(), Some(req));
                assert_eq!(c, ctx_out);
    
                let resp = resp.clone();
                let ctx_in = ctx_in.clone();
                
                mux.handle_resp(req_id, addr, resp, ctx_in).unwrap();
            }
        }.boxed();

        // Run using select
        // a will finish, b will poll forever
        let _ = block_on(future::select(a, b));

    }
}
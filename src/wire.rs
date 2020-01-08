
use std::collections::HashMap;
use std::marker::PhantomData;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::clone::Clone;
use std::fmt::Debug;
use std::pin::Pin;

use futures::prelude::*;
use futures::channel::{mpsc, oneshot};
use futures::task::{Context, Poll};
use async_trait::async_trait;

use crate::connector::Connector;

/// Wire provides an interconnect to support integration testing of Mux based implementations
pub struct Wire <ReqId, Target, Req, Resp, E, Ctx> {
    connectors: Arc<Mutex<HashMap<Target, WireMux<ReqId, Target, Req, Resp, E, Ctx>>>>,

    requests: Arc<Mutex<HashMap<(Target, Target, ReqId), oneshot::Sender<Resp>>>>,

    _e: PhantomData<E>, 
    _ctx: PhantomData<Ctx>,
}

impl <ReqId, Target, Req, Resp, E, Ctx> Clone for Wire<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Sync + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    fn clone(&self) -> Self {
        Wire {
            connectors: self.connectors.clone(),
            requests: self.requests.clone(),

            _e: PhantomData,
            _ctx: PhantomData,
        }
    }
}

impl <ReqId, Target, Req, Resp, E, Ctx> Wire<ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Sync + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    /// Create a new Wire interconnect
    pub fn new() -> Wire<ReqId, Target, Req, Resp, E, Ctx>  {
        Wire{
            connectors: Arc::new(Mutex::new(HashMap::new())),
            requests: Arc::new(Mutex::new(HashMap::new())),

            _e: PhantomData,
            _ctx: PhantomData,
        }
    }

    /// Create a new connector for the provided target address
    pub fn connector(&mut self, target: Target) -> WireMux<ReqId, Target, Req, Resp, E, Ctx> {
        let w = WireMux::new(self.clone(), target.clone());

        self.connectors.lock().unwrap().insert(target, w.clone());

        w
    }

    async fn request(&mut self, _ctx: Ctx, to: Target, from: Target, id: ReqId, req: Req) -> Result<Resp, ()> {
        // Fetch matching connector
        let mut conn = {
            let c = self.connectors.lock().unwrap();
            c.get(&to.clone()).unwrap().clone()
        };

        // Bind response channel
        let (tx, rx) = oneshot::channel();
        self.requests.lock().unwrap().insert((to, from.clone(), id.clone()), tx);

        // Forward request
        conn.send(from, id, req).await.unwrap();

        // Await response
        let res = rx.await.unwrap();

        Ok(res)
    }

    async fn respond(&mut self, _ctx: Ctx, to: Target, from: Target, id: ReqId, resp: Resp) -> Result<(), E> {
        let pending = self.requests.lock().unwrap().remove(&(from, to, id)).unwrap();
        
        pending.send(resp).unwrap();
        
        Ok(())
    }
}

pub struct WireMux<ReqId, Target, Req, Resp, E, Ctx> {
    addr: Target,

    connector: Wire<ReqId, Target, Req, Resp, E, Ctx>,

    receiver_tx: Arc<Mutex<mpsc::Sender<(Target, ReqId, Req)>>>,
    receiver_rx: Arc<Mutex<mpsc::Receiver<(Target, ReqId, Req)>>>,

    _e: PhantomData<E>, 
    _ctx: PhantomData<Ctx>,
}

impl <ReqId, Target, Req, Resp, E, Ctx> WireMux<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Sync + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    fn new(connector: Wire<ReqId, Target, Req, Resp, E, Ctx>, addr: Target) -> WireMux<ReqId, Target, Req, Resp, E, Ctx> {
        let (tx, rx) = mpsc::channel(0);

        WireMux{
            addr,
            connector,

            receiver_rx: Arc::new(Mutex::new(rx)),
            receiver_tx: Arc::new(Mutex::new(tx)),

            _e: PhantomData,
            _ctx: PhantomData,
        }
    }

    async fn send(&mut self, from: Target, id: ReqId, req: Req) -> Result<(), E> {
        let mut tx = self.receiver_tx.lock().unwrap().clone();
        
        match tx.send((from, id, req)).await {
            Ok(_) => (),
            Err(e) => panic!(e),
        };

        Ok(())
    }
}


impl <ReqId, Target, Req, Resp, E, Ctx> Clone for WireMux<ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Sync + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    fn clone(&self) -> Self {
        WireMux{
            addr: self.addr.clone(),
            connector: self.connector.clone(),

            receiver_rx: self.receiver_rx.clone(),
            receiver_tx: self.receiver_tx.clone(),

            _e: PhantomData,
            _ctx: PhantomData,
        }
    }
}

#[async_trait]
impl <ReqId, Target, Req, Resp, E, Ctx> Connector<ReqId, Target, Req, Resp, E, Ctx> for WireMux <ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Sync + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    // Send a request and receive a response or error at some time in the future
    async fn request(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, req: Req,
    ) -> Result<Resp, E> {

        // Send to connector and await response
        let res = match self.connector.request(ctx, target, self.addr.clone(), req_id, req).await {
            Ok(r) => r,
            Err(e) => panic!(e),
        };

        Ok(res)
    }

    // Respond to a received request
    async fn respond(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, resp: Resp,
    ) -> Result<(), E> {
        let mut conn = self.connector.clone();

        match conn.respond(ctx, target, self.addr.clone(), req_id, resp).await {
            Ok(_) => (),
            Err(e) => panic!(e),
        };

        Ok(())
    }
}

impl <ReqId, Target, Req, Resp, E, Ctx> Stream for WireMux <ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Hash + PartialEq + Eq + Sync + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    type Item = (Target, ReqId, Req);

    // Poll to receive pending requests
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let rx = self.receiver_rx.clone();
        let mut rx = rx.lock().unwrap();
        rx.poll_next_unpin(cx)
    }
}

#[cfg(test)]
mod tests {

    use futures::prelude::*;
    use futures::executor::block_on;

    use super::*;

    #[test]
    fn test_wiring() {
        let mut i: Wire<u16, u64, u32, u32, (), ()> = Wire::new();
        
        let mut c1 = i.connector(0x11);
        let mut c2 = i.connector(0x22);
        
        // c1 makes a request (and checks the response)
        let a = async move {
            let resp = c1.request((), 1, 0x22, 40).await.unwrap();
            assert_eq!(resp, 50);
        }.boxed();

        let b = async move {
            while let Some((from, id, val)) = c2.next().await {
                c2.respond((), id, from, val + 10).await.unwrap();
            }
        }.boxed();
        
        // Run using select
        // a will finish, b will poll forever
        let _ = block_on(future::select(a, b));

    }

}
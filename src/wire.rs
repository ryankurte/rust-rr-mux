
use std::collections::HashMap;
use std::marker::PhantomData;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::clone::Clone;
use std::fmt::Debug;

use futures::prelude::*;
use futures::future::{Future, lazy, ok};
use futures::sync::mpsc;
use futures::sync::oneshot;

use crate::connector::Connector;

pub struct Wire <ReqId, Target, Req, Resp, E, Ctx> {
    connectors: Arc<Mutex<HashMap<Target, WireMux<ReqId, Target, Req, Resp, E, Ctx>>>>,

    requests: Arc<Mutex<HashMap<(Target, Target, ReqId), oneshot::Sender<Resp>>>>,

    _req_id: PhantomData<ReqId>, 
    _req: PhantomData<Req>, 
    _resp: PhantomData<Resp>, 
    _e: PhantomData<E>, 
    _ctx: PhantomData<Ctx>,
}

impl <ReqId, Target, Req, Resp, E, Ctx> Clone for Wire<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    fn clone(&self) -> Self {
        Wire {
            connectors: self.connectors.clone(),
            requests: self.requests.clone(),

            _req_id: PhantomData,
            _req: PhantomData,
            _resp: PhantomData,
            _e: PhantomData,
            _ctx: PhantomData,
        }
    }
}

impl <ReqId, Target, Req, Resp, E, Ctx> Wire<ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    pub fn new() -> Wire<ReqId, Target, Req, Resp, E, Ctx>  {
        Wire{
            connectors: Arc::new(Mutex::new(HashMap::new())),
            requests: Arc::new(Mutex::new(HashMap::new())),

            _req_id: PhantomData,
            _req: PhantomData,
            _resp: PhantomData,
            _e: PhantomData,
            _ctx: PhantomData,
        }
    }

    pub fn connector(&mut self, target: Target) -> WireMux<ReqId, Target, Req, Resp, E, Ctx> {
        let w = WireMux::new(self.clone(), target.clone());

        self.connectors.lock().unwrap().insert(target, w.clone());

        w
    }

    fn request(&mut self, to: Target, from: Target, id: ReqId, req: Req) -> impl Future<Item=Resp, Error=()> {
        // Fetch matching connector
        let connectors = self.connectors.lock().unwrap();
        let mut conn = connectors.get(&to.clone()).unwrap().clone();
        
        // Bind response channel
        let (tx, rx) = oneshot::channel();
        self.requests.lock().unwrap().insert((to, from.clone(), id.clone()), tx);

        // Forward request
        conn.send(from, id, req).and_then(|_| {
            // Await response
            rx.map_err(|_e| () )
        })
    }

    fn respond(&mut self, to: Target, from: Target, id: ReqId, resp: Resp) -> impl Future<Item=(), Error=()> {
        let pending = self.requests.lock().unwrap().remove(&(from, to, id)).unwrap();
        pending.send(resp).map(|_| () ).map_err(|_| () );
        ok(())
    }
}

pub struct WireMux<ReqId, Target, Req, Resp, E, Ctx> {
    addr: Target,

    connector: Wire<ReqId, Target, Req, Resp, E, Ctx>,

    receiver_tx: Arc<Mutex<mpsc::Sender<(Target, ReqId, Req)>>>,
    receiver_rx: Arc<Mutex<mpsc::Receiver<(Target, ReqId, Req)>>>,

    _req_id: PhantomData<ReqId>, 
    _target: PhantomData<Target>,
    _req: PhantomData<Req>, 
    _resp: PhantomData<Resp>, 
    _e: PhantomData<E>, 
    _ctx: PhantomData<Ctx>,
}

impl <ReqId, Target, Req, Resp, E, Ctx> WireMux<ReqId, Target, Req, Resp, E, Ctx>
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Send + 'static,
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

            _req_id: PhantomData,
            _target: PhantomData,
            _req: PhantomData,
            _resp: PhantomData,
            _e: PhantomData,
            _ctx: PhantomData,
        }
    }

    fn send(&mut self, from: Target, id: ReqId, req: Req) -> impl Future<Item=(), Error=()> {
        let tx = self.receiver_tx.lock().unwrap().clone();
        tx.send((from, id, req)).map_err(|_e| () ).map(|_| () )
    }
}


impl <ReqId, Target, Req, Resp, E, Ctx> Clone for WireMux<ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Send + 'static,
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

            _req_id: PhantomData,
            _target: PhantomData,
            _req: PhantomData,
            _resp: PhantomData,
            _e: PhantomData,
            _ctx: PhantomData,
        }
    }
}
impl <ReqId, Target, Req, Resp, E, Ctx> Connector<ReqId, Target, Req, Resp, E, Ctx> for WireMux <ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Clone + Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Clone + Hash + PartialEq + Eq + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
        // Send a request and receive a response or error at some time in the future
    fn request(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, req: Req,
    ) -> Box<Future<Item = Resp, Error = E> + Send + 'static> {
        let mut conn = self.connector.clone();

        // Send to connector and await response
        Box::new(
            conn.request(target, self.addr.clone(), req_id, req)
            .map_err(|_e| panic!() )
        )
    }

    // Send a response message
    fn respond(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, resp: Resp,
    ) -> Box<Future<Item = (), Error = E> + Send + 'static> {
        let mut conn = self.connector.clone();

        Box::new(
            conn.respond(target, self.addr.clone(), req_id, resp)
            .map_err(|_e| panic!() )
        )
    }
}

impl <ReqId, Target, Req, Resp, E, Ctx> Stream for WireMux <ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: Hash + Eq + PartialEq + Debug + Send + 'static,
    Target: Hash + PartialEq + Eq + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    type Item = (Target, ReqId, Req);
    type Error = ();

    // Poll to read pending requests
    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let rx = self.receiver_rx.clone();
        let mut rx = rx.lock().unwrap();
        rx.poll()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::prelude::*;

    #[test]
    fn test_wiring() {
        let mut i: Wire<u16, u64, u32, u32, (), ()> = Wire::new();
        
        let mut c1 = i.connector(0x11);
        let mut c2 = i.connector(0x22);

        // c1 makes a request (and checks the response)
        let a = c1.request((), 1, 0x22, 40)
        .map(|resp| {
            assert_eq!(resp, 50);
        }).map_err(|_e| () );

        // c2 handles requests (and issues responses)
        let b = c2.clone().for_each(move |(from, id, val)| {
            c2.respond((), id, from, val + 10)
        }).map(|_| () ).map_err(|_e| () );
        
        // Run using select
        // a will finish, b will poll forever
        Future::select(a, b).wait();

    }

}
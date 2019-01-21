

use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::fmt::Debug;

use futures::prelude::*;
use futures::sync::{oneshot, oneshot::Sender as OneshotSender};

use crate::connector::Connector;

/// Muxed is a container for either a request or response message
#[derive(Debug, PartialEq)]
pub enum Muxed<REQ, RESP> {
    Request(REQ),
    Response(RESP),
}

impl <REQ, RESP> Muxed<REQ, RESP> {
    /// Fetch a request if muxed contains a request type
    pub fn req(self) -> Option<REQ> {
        match self {
            Muxed::Request(req) => Some(req),
            _ => None
        }
    }

    /// Fetch a response if muxed contains a response type
    pub fn resp(self) -> Option<RESP> {
        match self {
            Muxed::Response(resp) => Some(resp),
            _ => None
        }
    }
}

/// Mux is a futures based request response multiplexer
/// REQ_ID is the request REQ_ID type
/// TARGET is the target for the REQ or RESP to be sent to
/// REQ and RESP are the request and response messages
/// CTX is a a shared context
/// SENDER is a futures-based sender for sending messages
pub struct Mux<REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> {
    requests: Arc<Mutex<HashMap<REQ_ID, Box<OneshotSender<RESP>>>>>,
    sender: Arc<Mutex<SENDER>>,

    _addr: PhantomData<TARGET>,
    _req: PhantomData<REQ>,
    _err: PhantomData<ERR>,
    _ctx: PhantomData<CTX>,
}

impl <REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> Clone for Mux <REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync +Send + 'static,
    REQ: Debug + Sync + Send + 'static,
    RESP: Debug + Sync +Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync +Send + 'static,
    SENDER: FnMut(CTX, REQ_ID, Muxed<REQ, RESP>, TARGET) -> Box<Future<Item=(), Error=ERR> + Send + 'static> + Send + 'static,
{
    fn clone(&self) -> Self {
        Mux{
            requests: self.requests.clone(),
            sender: self.sender.clone(),
            _ctx: PhantomData,
            _addr: PhantomData,
            _req: PhantomData,
            _err: PhantomData,
        }
    }
}

impl <REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> Mux <REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync +Send + 'static,
    REQ: Debug + Sync + Send + 'static,
    RESP: Debug + Sync +Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync +Send + 'static,
    SENDER: FnMut(CTX, REQ_ID, Muxed<REQ, RESP>, TARGET) -> Box<Future<Item=(), Error=ERR> + Send + 'static> + Send + 'static,
{
    /// Create a new mux over the provided sender
    pub fn new(sender: SENDER) -> Mux<REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> {
        Mux{
            requests: Arc::new(Mutex::new(HashMap::new())), 
            sender: Arc::new(Mutex::new(sender)),
            _ctx: PhantomData,
            _addr: PhantomData,
            _req: PhantomData,
            _err: PhantomData,
        }
    }

    /// Handle a muxed received message
    /// This either returns a pending response or passes request messages on
    pub fn handle(&mut self, id: REQ_ID, addr: TARGET, message: Muxed<REQ, RESP>) -> Result<Option<(TARGET, REQ)>, ERR> {
        let r = match message {
            // Requests get passed through the mux
            Muxed::Request(req) => {
                Some((addr, req))
            },
            // Responses get matched with outstanding requests
            Muxed::Response(resp) => {
                self.handle_resp(id, addr, resp)?;
                None
            }
        };

        Ok(r)
    }

    /// Handle a pre-decoded response message
    pub fn handle_resp(&mut self, id: REQ_ID, _target: TARGET, resp: RESP) -> Result<(), ERR> {
        if let Some(ch) = self.requests.lock().unwrap().remove(&id) {
            ch.send(resp).unwrap();
        } else {
            info!("Response id: '{:?}', no request pending", id);
        }
        Ok(())
    }
}

impl <REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> Connector <REQ_ID, TARGET, REQ, RESP, ERR, CTX> for Mux <REQ_ID, TARGET, REQ, RESP, ERR, CTX, SENDER> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + Debug + Clone + Send +'static,
    TARGET: Debug + Send + 'static,
    REQ: Debug + Send + 'static,
    RESP: Debug + Send + 'static,
    ERR: Debug + Send + 'static,
    CTX: Clone + Send + 'static,
    SENDER: FnMut(CTX, REQ_ID, Muxed<REQ, RESP>, TARGET) -> Box<Future<Item=(), Error=ERR> + Send + 'static> + Send + 'static,
{

    /// Send and register a request
    fn request(&mut self, ctx: CTX, id: REQ_ID, addr: TARGET, req: REQ) -> Box<Future<Item=RESP, Error=ERR> + Send + 'static> {
        // Create future channel
        let (tx, rx) = oneshot::channel();

        // Save response to map
        self.requests.lock().unwrap().insert(id.clone(), Box::new(tx));

        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            let sender = &mut *sender.lock().unwrap();
            (sender)(ctx.clone(), id, Muxed::Request(req), addr)
        })
        .and_then(|_| {
            // Panic on future closed, this is probably not desirable
            // TODO: fix
            rx.map_err(|_e| panic!() )
        }))
    }

    fn respond(&mut self, ctx: CTX, id: REQ_ID, addr: TARGET, resp: RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static> {
        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            let sender = &mut *sender.lock().unwrap();
            (sender)(ctx, id, Muxed::Response(resp), addr)
        }))
    }
}

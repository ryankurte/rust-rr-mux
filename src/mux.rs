

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
/// ID is the request ID type
/// ADDR is the address for the REQ or RESP to be sent to
/// REQ and RESP are the request and response messages
/// SENDER is a futures-based sender for sending messages
#[derive(Clone)]
pub struct Mux<ID, ADDR, REQ, RESP, ERR, SENDER, CTX> {
    requests: Arc<Mutex<HashMap<ID, Box<OneshotSender<RESP>>>>>,
    sender: Arc<Mutex<SENDER>>,
    _addr: PhantomData<ADDR>,
    _req: PhantomData<REQ>,
    _err: PhantomData<ERR>,
    _ctx: PhantomData<CTX>,
}

impl <ID, ADDR, REQ, RESP, ERR, SENDER, CTX> Mux <ID, ADDR, REQ, RESP, ERR, SENDER, CTX> 
where
    ID: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + 'static,
    ADDR: Debug + Send + 'static,
    REQ: Debug + Send + 'static,
    RESP: Debug + Send + 'static,
    ERR: Debug + Send + 'static,
    SENDER: FnMut(CTX, ID, Muxed<REQ, RESP>, ADDR) -> Box<Future<Item=(), Error=ERR> + Send + 'static> + Send + 'static,
    CTX: Clone + Send + 'static,
{
    /// Create a new mux over the provided sender
    pub fn new(sender: SENDER) -> Mux<ID, ADDR, REQ, RESP, ERR, SENDER, CTX> {
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
    pub fn handle(&mut self, id: ID, addr: ADDR, message: Muxed<REQ, RESP>) -> Result<Option<(ADDR, REQ)>, ERR> {
        let r = match message {
            // Requests get passed through the mux
            Muxed::Request(req) => {
                Some((addr, req))
            },
            // Responses get matched with outstanding requests
            Muxed::Response(resp) => {
                self.handle_resp(id, addr, resp);
                None
            }
        };

        Ok(r)
    }

    /// Handle a pre-decoded response message
    pub fn handle_resp(&mut self, id: ID, addr: ADDR, resp: RESP) -> Result<(), ERR> {
        if let Some(ch) = self.requests.lock().unwrap().remove(&id) {
            ch.send(resp).unwrap();
        } else {
            info!("Response id: '{:?}', no request pending", id);
        }
        Ok(())
    }
}

impl <ID, ADDR, REQ, RESP, ERR, SENDER, CTX> Connector <ID, ADDR, REQ, RESP, ERR, CTX> for Mux <ID, ADDR, REQ, RESP, ERR, SENDER, CTX> 
where
    ID: std::cmp::Eq + std::hash::Hash + Debug + Clone + Send +'static,
    ADDR: Debug + Send + 'static,
    REQ: Debug + Send + 'static,
    RESP: Debug + Send + 'static,
    ERR: Debug + Send + 'static,
    SENDER: FnMut(CTX, ID, Muxed<REQ, RESP>, ADDR) -> Box<Future<Item=(), Error=ERR> + Send + 'static> + Send + 'static,
    CTX: Clone + Send + 'static,
{

    /// Send and register a request
    fn request(&mut self, ctx: CTX, id: ID, addr: ADDR, req: REQ) -> Box<Future<Item=RESP, Error=ERR> + Send + 'static> {
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

    fn respond(&mut self, ctx: CTX, id: ID, addr: ADDR, resp: RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static> {
        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            let sender = &mut *sender.lock().unwrap();
            (sender)(ctx, id, Muxed::Response(resp), addr)
        }))
    }
}

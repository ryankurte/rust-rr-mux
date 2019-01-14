

use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use std::marker::PhantomData;


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
    ADDR: std::fmt::Debug + 'static,
    REQ: std::fmt::Debug + 'static,
    RESP: std::fmt::Debug + 'static,
    ERR: std::fmt::Debug + 'static,
    SENDER: FnMut(&mut CTX, ID, Muxed<REQ, RESP>, ADDR) -> Box<Future<Item=(), Error=ERR>> + 'static,
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

    /// Handle a received message
    /// This either returns a pending response or passes request messages on
    pub fn handle(&mut self, id: ID, addr: ADDR, resp: Muxed<REQ, RESP>) -> Result<Option<(ADDR, REQ)>, ERR> {
        let r = match resp {
            // Requests get passed through the mux
            Muxed::Request(req) => {
                Some((addr, req))
            },
            // Responses get matched with outstanding requests
            Muxed::Response(resp) => {
                if let Some(ch) = self.requests.lock().unwrap().remove(&id) {
                    ch.send(resp).unwrap();
                } else {
                    info!("Response id: '{:?}', no request pending", id);
                }
                None
            }
        };

        Ok(r)
    }
}

impl <ID, ADDR, REQ, RESP, ERR, SENDER, CTX> Connector <ID, ADDR, REQ, RESP, ERR, CTX> for Mux <ID, ADDR, REQ, RESP, ERR, SENDER, CTX> 
where
    ID: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + 'static,
    ADDR: std::fmt::Debug + 'static,
    REQ: std::fmt::Debug + 'static,
    RESP: std::fmt::Debug + 'static,
    ERR: std::fmt::Debug + 'static,
    SENDER: FnMut(&mut CTX, ID, Muxed<REQ, RESP>, ADDR) -> Box<Future<Item=(), Error=ERR>> + 'static,
{

    /// Send and register a request
    fn request(&mut self, ctx: &mut CTX, id: ID, addr: ADDR, req: REQ) -> Box<Future<Item=RESP, Error=ERR>> {
        // Create future channel
        let (tx, rx) = oneshot::channel();

        // Save response to map
        self.requests.lock().unwrap().insert(id.clone(), Box::new(tx));

        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            let sender = &mut *sender.lock().unwrap();
            (sender)(ctx, id, Muxed::Request(req), addr)
        })
        .and_then(|_| {
            // Panic on future closed, this is probably not desirable
            // TODO: fix
            rx.map_err(|_e| panic!() )
        }))
    }

    fn respond(&mut self, ctx: &mut CTX, id: ID, addr: ADDR, resp: RESP) -> Box<Future<Item=(), Error=ERR>> {
        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            let sender = &mut *sender.lock().unwrap();
            (sender)(ctx, id, Muxed::Response(resp), addr)
        }))
    }
}

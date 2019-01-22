

use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::fmt::Debug;

use futures::prelude::*;
use futures::sync::{oneshot, oneshot::Sender as OneshotSender};
use futures::sync::mpsc::{channel, Sender as ChannelSender, Receiver as ChannelReceiver};


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
pub struct Mux<REQ_ID, TARGET, REQ, RESP, ERR, CTX> {
    requests: Arc<Mutex<HashMap<REQ_ID, Box<OneshotSender<RESP>>>>>,

    sender: ChannelSender<(REQ_ID, TARGET, Muxed<REQ, RESP>)>,
    receiver: Arc<Mutex<ChannelReceiver<(REQ_ID, TARGET, Muxed<REQ, RESP>)>>>,

    _addr: PhantomData<TARGET>,
    _req: PhantomData<REQ>,
    _err: PhantomData<ERR>,
    _ctx: PhantomData<CTX>,
}

impl <REQ_ID, TARGET, REQ, RESP, ERR, CTX> Clone for Mux <REQ_ID, TARGET, REQ, RESP, ERR, CTX> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync +Send + 'static,
    REQ: Debug + Sync + Send + 'static,
    RESP: Debug + Sync +Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync +Send + 'static,
{
    fn clone(&self) -> Self {
        Mux{
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

impl <REQ_ID, TARGET, REQ, RESP, ERR, CTX> Mux <REQ_ID, TARGET, REQ, RESP, ERR, CTX> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync +Send + 'static,
    REQ: Debug + Sync + Send + 'static,
    RESP: Debug + Sync +Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync +Send + 'static,
{
    /// Create a new mux over the provided sender
    pub fn new() -> Mux<REQ_ID, TARGET, REQ, RESP, ERR, CTX> {
        let (tx, rx) = channel(0);

        Mux{
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

impl <REQ_ID, TARGET, REQ, RESP, ERR, CTX> Connector <REQ_ID, TARGET, REQ, RESP, ERR, CTX> for Mux <REQ_ID, TARGET, REQ, RESP, ERR, CTX> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + Debug + Clone + Send +'static,
    TARGET: Debug + Send + 'static,
    REQ: Debug + Send + 'static,
    RESP: Debug + Send + 'static,
    ERR: Debug + Send + 'static,
    CTX: Clone + Send + 'static,
{

    /// Send and register a request
    fn request(&mut self, _ctx: CTX, id: REQ_ID, addr: TARGET, req: REQ) -> Box<Future<Item=RESP, Error=ERR> + Send + 'static> {
        // Create future channel
        let (tx, rx) = oneshot::channel();

        // Save response to map
        self.requests.lock().unwrap().insert(id.clone(), Box::new(tx));

        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            sender.send((id, addr, Muxed::Request(req))).map(|_r| () ).map_err(|_e| panic!() )
        })
        .and_then(|_| {
            // Panic on future closed, this is probably not desirable
            // TODO: fix this
            rx.map_err(|_e| panic!() )
        }))
    }

    fn respond(&mut self, _ctx: CTX, id: REQ_ID, addr: TARGET, resp: RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static> {
        // Send request and return channel future
        let sender = self.sender.clone();
        Box::new(futures::lazy(move || {
            sender.send((id, addr, Muxed::Response(resp))).map(|_r| () ).map_err(|_e| panic!() )
        }))
    }
}

impl <REQ_ID, TARGET, REQ, RESP, ERR, CTX> Stream for Mux <REQ_ID, TARGET, REQ, RESP, ERR, CTX> {
    type Item = (REQ_ID, TARGET, Muxed<REQ, RESP>);
    type Error = ();

    // Poll to read pending requests
    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        self.receiver.lock().unwrap().poll()
    }
}

use std::sync::{Mutex, Arc};
use std::collections::VecDeque;
use std::fmt::Debug;
use std::time::Duration;

use futures::prelude::*;
use futures::future::{ok, err, Either};

use derive_builder::Builder;

use crate::mux::Muxed;
use crate::connector::Connector;

/// MockRequest is a mocked request expectation with a provided response
#[derive(Debug, PartialEq, Builder)]
pub struct MockRequest<ADDR, REQ, RESP, ERR> {
    to: ADDR,
    req: REQ,
    resp: Result<RESP, ERR>,
    delay: Option<Duration>,
}

impl <ADDR, REQ, RESP, ERR> MockRequest<ADDR, REQ, RESP, ERR> {
    /// Create a new mock request.
    /// You probably want to use MockTransaction::request instead of constructing this directly
    pub fn new(to: ADDR, req: REQ, resp: Result<RESP, ERR>) -> MockRequest<ADDR, REQ, RESP, ERR> {
        MockRequest{to, req, resp, delay: None}
    }
}

/// MockResponse is a mocked response expectation
#[derive(Debug, PartialEq, Builder)]
pub struct MockResponse<ADDR, RESP, ERR> {
    to: ADDR,
    resp: RESP,
    err: Option<ERR>,
}

impl <ADDR, RESP, ERR> MockResponse<ADDR, RESP, ERR> {
    /// Create a new mock response.
    /// You probably want to use MockTransaction::response instead of constructing this directly
    pub fn new(to: ADDR, resp: RESP, err: Option<ERR>) -> MockResponse<ADDR, RESP, ERR> {
        MockResponse{to, resp: resp, err: err}
    }

    pub fn with_error(mut self, err: ERR) -> Self {
        self.err = Some(err);
        self
    }
}

// MockTransaction is a transaction expectation
pub type MockTransaction<ADDR, REQ, RESP, ERR> = Muxed<MockRequest<ADDR, REQ, RESP, ERR>, MockResponse<ADDR, RESP, ERR>>;

impl <ADDR, REQ, RESP, ERR> MockTransaction <ADDR, REQ, RESP, ERR> {

    /// Create a mock request -> response transaction
    pub fn request<_REQ: Into<REQ>, _RESP: Into<RESP>, _ERR: Into<ERR>>(to: ADDR, req: _REQ, resp: Result<_RESP, _ERR>) -> MockTransaction <ADDR, REQ, RESP, ERR> {
        Muxed::Request(MockRequest::new(to, req.into(), resp.map(|v| v.into()).map_err(|e| e.into())))
    }

    /// Create a mock response transaction
    pub fn response<_RESP: Into<RESP>, _ERR: Into<ERR>>(to: ADDR, resp: _RESP, err: Option<_ERR>) -> MockTransaction <ADDR, REQ, RESP, ERR> {
        Muxed::Response(MockResponse::new(to, resp.into(), err.map(|v| v.into())))
    }
}

/// MockConnector provides an expectation based mock connector implementation
/// to simplify writing tests against modules using the Connector abstraction.
#[derive(Clone)]
pub struct MockConnector<ADDR, REQ, RESP, ERR> 
{
    transactions: Arc<Mutex<VecDeque<MockTransaction<ADDR, REQ, RESP, ERR>>>>,
}

impl <ADDR, REQ, RESP, ERR> MockConnector <ADDR, REQ, RESP, ERR> 
where
    ADDR: PartialEq + Debug + 'static,
    REQ: PartialEq + Debug + 'static,
    RESP: PartialEq + Debug + 'static,
    ERR: PartialEq + Debug + 'static,
{
    /// Create a new mock connector
    pub fn new() -> MockConnector<ADDR, REQ, RESP, ERR> {
        MockConnector{transactions: Arc::new(Mutex::new(VecDeque::new()))}
    }

    /// Set expectations on the connector
    pub fn expect<E>(mut self, transactions: E) -> Self
    where
        E: Into<VecDeque<MockTransaction<ADDR, REQ, RESP, ERR>>>,
    {
        *self.transactions.lock().unwrap() = transactions.into();

        self
    }

    /// Finalise expectations on the connector
    pub fn finalise(&mut self) {
        let transactions: Vec<_> = self.transactions.lock().unwrap().drain(..).collect();
        let expectations = Vec::<MockTransaction<ADDR, REQ, RESP, ERR>>::new();
        assert_eq!(expectations, transactions, "not all transactions have been evaluated");
    }
}

impl <ID, ADDR, REQ, RESP, ERR, CTX> Connector <ID, ADDR, REQ, RESP, ERR, CTX> for MockConnector <ADDR, REQ, RESP, ERR> 
where
    ID: PartialEq + Debug + Send + 'static,
    ADDR: PartialEq + Debug + Send + 'static,
    REQ: PartialEq + Debug + Send + 'static,
    RESP: PartialEq + Debug + Send + 'static,
    ERR: PartialEq + Debug + Send + 'static,
{
    /// Make a request and return the pre-set response
    /// This checks the request against the specified expectations
    fn request(&mut self, _ctx: CTX, _id: ID, addr: ADDR, req: REQ) -> Box<Future<Item=RESP, Error=ERR> + Send + 'static> {
        
        let mut transactions = self.transactions.lock().unwrap();
        
        let transaction = transactions.pop_front()
            .expect(&format!("request error, no more transactions available (request: {:?})", req));
        let request = transaction.req().expect("expected request");

        assert_eq!(request.to, addr, "destination mismatch");
        assert_eq!(request.req, req, "request mismatch");

        Box::new(match request.resp {
            Ok(r) => ok(r),
            Err(e) => err(e),
        })
    }

    /// Make a response
    /// This checks the response against provided expectations
    fn respond(&mut self, _ctx: CTX, _id: ID, addr: ADDR, resp: RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static> {
        let mut transactions = self.transactions.lock().unwrap();

        let transaction = transactions.pop_front()
            .expect(&format!("response error, no more transactions available (response: {:?})", resp));
        let response = transaction.resp().expect("expected response");

        assert_eq!(response.to, addr, "destination mismatch");
        assert_eq!(response.resp, resp, "request mismatch");
        
        Box::new(match response.err {
            Some(e) => err(e),
            None => ok(()),
        })
    }
}
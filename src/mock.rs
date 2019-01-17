
use std::sync::{Mutex, Arc};
use std::collections::VecDeque;
use std::fmt::Debug;

use futures::prelude::*;
use futures::future;

use derive_builder::Builder;

use crate::mux::Muxed;
use crate::connector::Connector;

/// MockRequest is a mocked request expectation with a provided response
#[derive(Debug, PartialEq, Builder)]
pub struct MockRequest<ID, ADDR, REQ, RESP, ERR> {
    id: Option<ID>,
    to: ADDR,
    req: REQ,
    from: ADDR,
    resp: RESP,
    err: Option<ERR>,
}

impl <ID, ADDR, REQ, RESP, ERR> MockRequest<ID, ADDR, REQ, RESP, ERR> {
    pub fn new(to: ADDR, from: ADDR, req: REQ, resp: RESP) -> MockRequest<ID, ADDR, REQ, RESP, ERR> {
        MockRequest{id: None, to, req, from, resp, err: None}
    }
}

/// MockResponse is a mocked response expectation
#[derive(Debug, PartialEq, Builder)]
pub struct MockResponse<ID, ADDR, RESP, ERR> {
    id: Option<ID>,
    to: ADDR,
    from: ADDR,
    resp: RESP,
    err: Option<ERR>,
}

impl <ID, ADDR, RESP, ERR> MockResponse<ID, ADDR, RESP, ERR> {
    pub fn new(to: ADDR, from: ADDR, resp: RESP) -> MockResponse<ID, ADDR, RESP, ERR> {
        MockResponse{id: None, to, from, resp, err: None}
    }
}

// MockTransaction is a transaction expectation
pub type MockTransaction<ID, ADDR, REQ, RESP, ERR> = Muxed<MockRequest<ID, ADDR, REQ, RESP, ERR>, MockResponse<ID, ADDR, RESP, ERR>>;

/// MockConnector provides an expectation based mock connector implementation
#[derive(Clone)]
pub struct MockConnector<ID, ADDR, REQ, RESP, ERR> 
{
    transactions: Arc<Mutex<VecDeque<MockTransaction<ID, ADDR, REQ, RESP, ERR>>>>,
}

impl <ID, ADDR, REQ, RESP, ERR> MockConnector <ID, ADDR, REQ, RESP, ERR> 
where
    ID: PartialEq + Debug + 'static,
    ADDR: PartialEq + Debug + 'static,
    REQ: PartialEq + Debug + 'static,
    RESP: PartialEq + Debug + 'static,
    ERR: PartialEq + Debug + 'static,
{
    /// Create a new mock connector
    pub fn new() -> MockConnector<ID, ADDR, REQ, RESP, ERR> {
        MockConnector{transactions: Arc::new(Mutex::new(VecDeque::new()))}
    }

    /// Set expectations on the connector
    pub fn expect<E>(&mut self, transactions: E)
    where
        E: Into<VecDeque<MockTransaction<ID, ADDR, REQ, RESP, ERR>>>,
    {
        *self.transactions.lock().unwrap() = transactions.into();
    }

    /// Finalise expectations on the connector
    pub fn finalise(&mut self) {
        let transactions: Vec<_> = self.transactions.lock().unwrap().drain(..).collect();
        let expectations = Vec::<MockTransaction<ID, ADDR, REQ, RESP, ERR>>::new();
        assert_eq!(expectations, transactions, "not all transactions have been evaluated");
    }
}

impl <ID, ADDR, REQ, RESP, ERR, CTX> Connector <ID, ADDR, REQ, RESP, ERR, CTX> for MockConnector <ID, ADDR, REQ, RESP, ERR> 
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

        Box::new(future::ok(request.resp))
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
        
        Box::new(future::ok(()))
    }
}
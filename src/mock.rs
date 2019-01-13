
use std::sync::{Mutex, Arc};
use std::collections::VecDeque;
use std::fmt::Debug;

use futures::prelude::*;
use futures::future;

use crate::mux::Muxed;
use crate::connector::Connector;

/// MockResponse is a mocked request expectation
#[derive(Debug, PartialEq)]
pub struct MockRequest<ID, ADDR, REQ, RESP, ERR> {
    id: ID,
    to: ADDR,
    req: REQ,
    from: ADDR,
    resp: RESP,
    err: ERR,
}

/// MockResponse is a mocked response expectation
#[derive(Debug, PartialEq)]
pub struct MockResponse<ID, ADDR, RESP, ERR> {
    id: ID,
    to: ADDR,
    resp: RESP,
    err: ERR,
}

// MockTransaction is a transaction expectation
pub type MockTransaction<ID, ADDR, REQ, RESP, ERR> = Muxed<MockRequest<ID, ADDR, REQ, RESP, ERR>, MockResponse<ID, ADDR, RESP, ERR>>;

/// MockConnector provides an expectation based mock connector implementation
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

impl <ID, ADDR, REQ, RESP, ERR> Connector <ID, ADDR, REQ, RESP, ERR> for MockConnector <ID, ADDR, REQ, RESP, ERR> 
where
    ID: PartialEq + Debug + 'static,
    ADDR: PartialEq + Debug + 'static,
    REQ: PartialEq + Debug + 'static,
    RESP: PartialEq + Debug + 'static,
    ERR: PartialEq + Debug + 'static,
{
    /// Make a request and return the pre-set response
    /// This checks the request against the specified expectations
    fn request(&mut self, _id: ID, addr: ADDR, req: REQ) -> Box<Future<Item=RESP, Error=ERR>> {
        
        let mut transactions = self.transactions.lock().unwrap();
        
        let transaction = transactions.pop_front().expect("no more transactions available");
        let request = transaction.req().expect("expected request");

        assert_eq!(request.to, addr, "destination mismatch");
        assert_eq!(request.req, req, "request mismatch");

        Box::new(future::ok(request.resp))
    }

    /// Make a response
    /// This checks the response against provided expectations
    fn respond(&mut self, _id: ID, addr: ADDR, resp: RESP) -> Box<Future<Item=(), Error=ERR>> {
        let mut transactions = self.transactions.lock().unwrap();

        let transaction = transactions.pop_front().expect("no more transactions available");
        let response = transaction.resp().expect("expected response");

        assert_eq!(response.to, addr, "destination mismatch");
        assert_eq!(response.resp, resp, "request mismatch");
        
        Box::new(future::ok(()))
    }
}
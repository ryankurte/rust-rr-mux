use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::{err, ok};
use futures::prelude::*;

use derive_builder::Builder;

use crate::connector::Connector;
use crate::muxed::Muxed;

/// MockRequest is a mocked request expectation with a provided response
#[derive(Debug, PartialEq, Builder)]
pub struct MockRequest<Addr, Req, Resp, E> {
    to: Addr,
    req: Req,
    resp: Result<Resp, E>,
    delay: Option<Duration>,
}

impl<Addr, Req, Resp, E> MockRequest<Addr, Req, Resp, E> {
    /// Create a new mock request.
    /// You probably want to use MockTransaction::request instead of constructing this directly
    pub fn new(to: Addr, req: Req, resp: Result<Resp, E>) -> MockRequest<Addr, Req, Resp, E> {
        MockRequest {
            to,
            req,
            resp,
            delay: None,
        }
    }
}

/// MockResponse is a mocked response expectation
#[derive(Debug, PartialEq, Builder)]
pub struct MockResponse<Addr, Resp, E> {
    to: Addr,
    resp: Resp,
    err: Option<E>,
}

impl<Addr, Resp, E> MockResponse<Addr, Resp, E> {
    /// Create a new mock response.
    /// You probably want to use MockTransaction::response instead of constructing this directly
    pub fn new(to: Addr, resp: Resp, err: Option<E>) -> MockResponse<Addr, Resp, E> {
        MockResponse {
            to,
            resp: resp,
            err: err,
        }
    }

    pub fn with_error(mut self, err: E) -> Self {
        self.err = Some(err);
        self
    }
}

// MockTransaction is a transaction expectation
pub type MockTransaction<Addr, Req, Resp, E> =
    Muxed<MockRequest<Addr, Req, Resp, E>, MockResponse<Addr, Resp, E>>;

impl<Addr, Req, Resp, E> MockTransaction<Addr, Req, Resp, E> {
    /// Create a mock request -> response transaction
    pub fn request(
        to: Addr, req: Req, resp: Result<Resp, E>,
    ) -> MockTransaction<Addr, Req, Resp, E> {
        Muxed::Request(MockRequest::new(to, req, resp))
    }

    /// Create a mock response transaction
    pub fn response(to: Addr, resp: Resp, err: Option<E>) -> MockTransaction<Addr, Req, Resp, E> {
        Muxed::Response(MockResponse::new(to, resp, err))
    }
}

/// MockConnector provides an expectation based mock connector implementation
/// to simplify writing tests against modules using the Connector abstraction.
pub struct MockConnector<Addr, Req, Resp, E, Ctx> {
    transactions: Arc<Mutex<VecDeque<MockTransaction<Addr, Req, Resp, E>>>>,
    _ctx: PhantomData<Ctx>,
}

impl<Addr, Req, Resp, E, Ctx> Clone for MockConnector<Addr, Req, Resp, E, Ctx> {
    fn clone(&self) -> Self {
        MockConnector {
            transactions: self.transactions.clone(),
            _ctx: PhantomData,
        }
    }
}

impl<Addr, Req, Resp, E, Ctx> MockConnector<Addr, Req, Resp, E, Ctx>
where
    Addr: PartialEq + Debug + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    /// Create a new mock connector
    pub fn new() -> MockConnector<Addr, Req, Resp, E, Ctx> {
        MockConnector {
            transactions: Arc::new(Mutex::new(VecDeque::new())),
            _ctx: PhantomData,
        }
    }

    /// Set expectations on the connector
    pub fn expect<T>(&mut self, transactions: T) -> Self
    where
        T: Into<VecDeque<MockTransaction<Addr, Req, Resp, E>>>,
    {
        *self.transactions.lock().unwrap() = transactions.into();

        self.clone()
    }

    /// Finalise expectations on the connector
    pub fn finalise(&mut self) {
        let transactions: Vec<_> = self.transactions.lock().unwrap().drain(..).collect();
        let expectations = Vec::<MockTransaction<Addr, Req, Resp, E>>::new();
        assert_eq!(
            expectations, transactions,
            "not all transactions have been evaluated"
        );
    }
}

impl<Id, Addr, Req, Resp, E, Ctx> Connector<Id, Addr, Req, Resp, E, Ctx>
    for MockConnector<Addr, Req, Resp, E, Ctx>
where
    Id: PartialEq + Debug + Send + 'static,
    Addr: PartialEq + Debug + Send + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
{
    /// Make a request and return the pre-set response
    /// This checks the request against the specified expectations
    fn request(
        &mut self, _ctx: Ctx, _id: Id, addr: Addr, req: Req,
    ) -> Box<Future<Item = Resp, Error = E> + Send + 'static> {
        let mut transactions = self.transactions.lock().unwrap();

        let transaction = transactions.pop_front().expect(&format!(
            "request error, no more transactions available (request: {:?})",
            req
        ));
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
    fn respond(
        &mut self, _ctx: Ctx, _id: Id, addr: Addr, resp: Resp,
    ) -> Box<Future<Item = (), Error = E> + Send + 'static> {
        let mut transactions = self.transactions.lock().unwrap();

        let transaction = transactions.pop_front().expect(&format!(
            "response error, no more transactions available (response: {:?})",
            resp
        ));
        let response = transaction.resp().expect("expected response");

        assert_eq!(response.to, addr, "destination mismatch");
        assert_eq!(response.resp, resp, "request mismatch");

        Box::new(match response.err {
            Some(e) => err(e),
            None => ok(()),
        })
    }
}



use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::fmt::Debug;

use futures::prelude::*;
use futures::sync::{oneshot, oneshot::Sender as OneshotSender};

use crate::connector::Connector;
use crate::mux::Muxed;

pub struct Mapped<BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, TX, RX> 
{
    conn: CONN,

    _req_id: PhantomData<REQ_ID>,
    _target: PhantomData<TARGET>,

    _base_req: PhantomData<BASE_REQ>,
    _base_resp: PhantomData<BASE_RESP>,
    _mapped_req: PhantomData<MAPPED_REQ>,
    _mapped_resp: PhantomData<MAPPED_RESP>,

    _err: PhantomData<ERR>,
    _ctx: PhantomData<CTX>,

    tx: TX,
    rx: RX,
}

impl <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, TX, RX> Mapped <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, TX, RX> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync + Send + 'static,
    BASE_REQ: Debug + Sync + Send + 'static,
    BASE_RESP: Debug + Sync + Send + 'static,
    MAPPED_REQ: Debug + Send + 'static,
    MAPPED_RESP: Debug + Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync + Send + 'static,
    TX: FnMut(Muxed<MAPPED_REQ, MAPPED_RESP>) -> Muxed<BASE_REQ, BASE_RESP> + Sync + 'static,
    RX: FnMut(Muxed<BASE_REQ, BASE_RESP>) -> Muxed<MAPPED_REQ, MAPPED_RESP> + Sync + 'static,
    CONN: Connector<REQ_ID, TARGET, BASE_REQ, BASE_RESP, ERR, CTX> + Sync + 'static
{
    pub fn new(conn: CONN, tx: TX, rx: RX) -> Mapped<BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, TX, RX> {
        Mapped{
            conn,
            tx,
            rx,

            _req_id: PhantomData,
            _target: PhantomData,

            _base_req: PhantomData,
            _base_resp: PhantomData,
            _mapped_req: PhantomData,
            _mapped_resp: PhantomData,

            _err: PhantomData,
            _ctx: PhantomData,
        }
    }
}

impl <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, TX, RX> Connector<REQ_ID, TARGET, MAPPED_REQ, MAPPED_RESP, ERR, CTX> for Mapped <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, TX, RX> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync + Send + 'static,
    BASE_REQ: Debug + Sync + Send + 'static,
    BASE_RESP: Debug + Sync + Send + 'static,
    MAPPED_REQ: Debug + Send + 'static,
    MAPPED_RESP: Debug + Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync + Send + 'static,
    TX: FnMut(Muxed<MAPPED_REQ, MAPPED_RESP>) -> Muxed<BASE_REQ, BASE_RESP> + Sync + Send + 'static,
    RX: FnMut(Muxed<BASE_REQ, BASE_RESP>) -> Muxed<MAPPED_REQ, MAPPED_RESP> + Sync + Send + Clone + 'static,
    CONN: Connector<REQ_ID, TARGET, BASE_REQ, BASE_RESP, ERR, CTX> + Sync + 'static,
{
    fn request(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, req: MAPPED_REQ) -> Box<Future<Item=MAPPED_RESP, Error=ERR> + Send + 'static> {
        let mut rx = (self.rx.clone());
        
        let req = (self.tx)(Muxed::Request(req));
        Box::new(self.conn.request(ctx, req_id, target, req.req().unwrap()).map(move |resp| {
            (rx)(Muxed::Response(resp)).resp().unwrap()
        }))
    }

    fn respond(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, resp: MAPPED_RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static> {       
        let resp = (self.tx)(Muxed::Response(resp));
        Box::new(self.conn.respond(ctx, req_id, target, resp.resp().unwrap()))
    }
}

#[cfg(test)]
mod tests {

    use crate::mux::{Mux, Muxed};
    use crate::mock::{MockConnector, MockTransaction};
    use crate::mapped::Mapped;

    use futures::future::Future;
    use crate::connector::Connector;

    #[derive(PartialEq, Debug, Clone)]
    struct A(u64);
    #[derive(PartialEq, Debug, Clone)]
    struct B(u64);

    #[test]
    fn test_mapping() {
        // Create mock mux
        let mut m = MockConnector::<u16, A, A, ()>::new();

        // Build wrapper
        let mut w = Mapped::<A, A, B, B, u64, u16, (), (), _, _, _>::new(m.clone(), |outgoing| {
            match outgoing {
                Muxed::Request(req) => Muxed::Request(A(req.0)),
                Muxed::Response(resp) => Muxed::Response(A(resp.0)),
            }
        }, |incoming| {
            match incoming {
                Muxed::Request(req) => Muxed::Request(B(req.0)),
                Muxed::Response(resp) => Muxed::Response(B(resp.0)),
            }
        });

        m.expect(vec![
            MockTransaction::request(1, A(0), Ok(A(1))),
            MockTransaction::response(1, A(2), None),
        ]);

        let resp = w.request((), 0, 1, B(0)).wait().unwrap();
        assert_eq!(resp, B(1));

        w.respond((), 0, 1, B(2)).wait().unwrap();

        m.finalise();
    }
}

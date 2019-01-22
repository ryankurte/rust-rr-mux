

use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::fmt::Debug;

use futures::prelude::*;
use futures::sync::{oneshot, oneshot::Sender as OneshotSender};

use crate::connector::Connector;
use crate::muxed::Muxed;

/// Mapper implements mappings for outgoing and incoming Muxed<Request, Response> pairs.
pub trait Mapper {
    type Original;
    type Mapped;

    fn outgoing(&self, m: Self::Mapped) -> Self::Original;
    fn incoming(&self, o: Self::Original) -> Self::Mapped;
}

/// Mapped wraps a connector type with a Mapper implementation
pub struct Mapped<BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, MAPPER> 
{
    conn: CONN,
    mapper: MAPPER,

    _req_id: PhantomData<REQ_ID>,
    _target: PhantomData<TARGET>,

    _base_req: PhantomData<BASE_REQ>,
    _base_resp: PhantomData<BASE_RESP>,
    _mapped_req: PhantomData<MAPPED_REQ>,
    _mapped_resp: PhantomData<MAPPED_RESP>,

    _err: PhantomData<ERR>,
    _ctx: PhantomData<CTX>,
}

impl <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, MAPPER> Mapped <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, MAPPER> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync + Send + 'static,
    BASE_REQ: Debug + Sync + Send + 'static,
    BASE_RESP: Debug + Sync + Send + 'static,
    MAPPED_REQ: Debug + Send + 'static,
    MAPPED_RESP: Debug + Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync + Send + 'static,
    CONN: Connector<REQ_ID, TARGET, BASE_REQ, BASE_RESP, ERR, CTX> + Sync + 'static,
    MAPPER: Mapper<Original=Muxed<BASE_REQ, BASE_RESP>, Mapped=Muxed<MAPPED_REQ, MAPPED_RESP>> +  Clone + Sync + Send + 'static,
{
    pub fn new(conn: CONN, mapper: MAPPER) -> Mapped<BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, MAPPER> {
        Mapped{
            conn,
            mapper,

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

impl <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, MAPPER> Connector<REQ_ID, TARGET, MAPPED_REQ, MAPPED_RESP, ERR, CTX> for Mapped <BASE_REQ, BASE_RESP, MAPPED_REQ, MAPPED_RESP, REQ_ID, TARGET, ERR, CTX, CONN, MAPPER> 
where
    REQ_ID: std::cmp::Eq + std::hash::Hash + Debug + Clone + Sync + Send + 'static,
    TARGET: Debug + Sync + Send + 'static,
    BASE_REQ: Debug + Sync + Send + 'static,
    BASE_RESP: Debug + Sync + Send + 'static,
    MAPPED_REQ: Debug + Send + 'static,
    MAPPED_RESP: Debug + Send + 'static,
    ERR: Debug + Sync + Send + 'static,
    CTX: Clone + Sync + Send + 'static,
    CONN: Connector<REQ_ID, TARGET, BASE_REQ, BASE_RESP, ERR, CTX> + Sync + 'static,
    MAPPER: Mapper<Original=Muxed<BASE_REQ, BASE_RESP>, Mapped=Muxed<MAPPED_REQ, MAPPED_RESP>> + Clone + Sync + Send + 'static,
{
    fn request(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, req: MAPPED_REQ) -> Box<Future<Item=MAPPED_RESP, Error=ERR> + Send + 'static> { 
        let m = self.mapper.clone();

        let req = self.mapper.outgoing(Muxed::Request(req));
        Box::new(self.conn.request(ctx, req_id, target, req.req().unwrap()).map(move |resp| {
            m.incoming(Muxed::Response(resp)).resp().unwrap()
        }))
    }

    fn respond(&mut self, ctx: CTX, req_id: REQ_ID, target: TARGET, resp: MAPPED_RESP) -> Box<Future<Item=(), Error=ERR> + Send + 'static> {       
        let resp = self.mapper.outgoing(Muxed::Response(resp));
        Box::new(self.conn.respond(ctx, req_id, target, resp.resp().unwrap()))
    }
}


#[cfg(test)]
mod tests {

    use crate::mux::Mux;
    use crate::muxed::Muxed;
    use crate::mock::{MockConnector, MockTransaction};
    use crate::mapped::{Mapped, Mapper};

    use futures::future::Future;
    use crate::connector::Connector;

    #[derive(PartialEq, Debug, Clone)]
    struct A(u64);
    #[derive(PartialEq, Debug, Clone)]
    struct B(u64);

    #[derive(Clone)]
    struct MapImpl ();

    impl Mapper for MapImpl {
        type Original = Muxed<A, A>;
        type Mapped = Muxed<B, B>;

        fn outgoing(&self, m: Self::Mapped) -> Self::Original {
            match m {
                Muxed::Request(req) => Muxed::Request(A(req.0)),
                Muxed::Response(resp) => Muxed::Response(A(resp.0)),
            }
        }
        fn incoming(&self, o: Self::Original) -> Self::Mapped {
            match o {
                Muxed::Request(req) => Muxed::Request(B(req.0)),
                Muxed::Response(resp) => Muxed::Response(B(resp.0)),
            }
        }
    }

    #[test]
    fn test_mapping() {
        // Create mock mux
        let mut m = MockConnector::<u16, A, A, ()>::new();

        // Build wrapper
        let mut w = Mapped::<A, A, B, B, u64, u16, (), (), MockConnector::<u16, A, A, ()>, MapImpl>::new(m.clone(), MapImpl());

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


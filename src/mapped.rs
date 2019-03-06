use std::fmt::Debug;
use std::marker::PhantomData;

use futures::prelude::*;

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
pub struct Mapped<BaseReq, BaseResp, MappedReq, MappedResp, ReqId, Target, E, Ctx, Conn, M> {
    conn: Conn,
    mapper: M,

    _req_id: PhantomData<ReqId>,
    _target: PhantomData<Target>,

    _base_req: PhantomData<BaseReq>,
    _base_resp: PhantomData<BaseResp>,
    _mapped_req: PhantomData<MappedReq>,
    _mapped_resp: PhantomData<MappedResp>,

    _err: PhantomData<E>,
    _ctx: PhantomData<Ctx>,
}

impl<BaseReq, BaseResp, MappedReq, MappedResp, ReqId, Target, E, Ctx, Conn, M>
    Mapped<BaseReq, BaseResp, MappedReq, MappedResp, ReqId, Target, E, Ctx, Conn, M>
where
    ReqId: std::cmp::Eq + std::hash::Hash + Debug + Clone + Sync + Send + 'static,
    Target: Debug + Sync + Send + 'static,
    BaseReq: Debug + Sync + Send + 'static,
    BaseResp: Debug + Sync + Send + 'static,
    MappedReq: Debug + Send + 'static,
    MappedResp: Debug + Send + 'static,
    E: Debug + Sync + Send + 'static,
    Ctx: Clone + Sync + Send + 'static,
    Conn: Connector<ReqId, Target, BaseReq, BaseResp, E, Ctx> + Sync + 'static,
    M: Mapper<Original = Muxed<BaseReq, BaseResp>, Mapped = Muxed<MappedReq, MappedResp>>
        + Clone
        + Sync
        + Send
        + 'static,
{
    pub fn new(
        conn: Conn, mapper: M,
    ) -> Mapped<BaseReq, BaseResp, MappedReq, MappedResp, ReqId, Target, E, Ctx, Conn, M> {
        Mapped {
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

impl<BaseReq, BaseResp, MappedReq, MappedResp, ReqId, Target, E, Ctx, Conn, M>
    Connector<ReqId, Target, MappedReq, MappedResp, E, Ctx>
    for Mapped<BaseReq, BaseResp, MappedReq, MappedResp, ReqId, Target, E, Ctx, Conn, M>
where
    ReqId: std::cmp::Eq + std::hash::Hash + Debug + Clone + Sync + Send + 'static,
    Target: Debug + Sync + Send + 'static,
    BaseReq: Debug + Sync + Send + 'static,
    BaseResp: Debug + Sync + Send + 'static,
    MappedReq: Debug + Send + 'static,
    MappedResp: Debug + Send + 'static,
    E: Debug + Sync + Send + 'static,
    Ctx: Clone + Sync + Send + 'static,
    Conn: Connector<ReqId, Target, BaseReq, BaseResp, E, Ctx> + Sync + 'static,
    M: Mapper<Original = Muxed<BaseReq, BaseResp>, Mapped = Muxed<MappedReq, MappedResp>>
        + Clone
        + Sync
        + Send
        + 'static,
{
    fn request(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, req: MappedReq,
    ) -> Box<Future<Item = (MappedResp, Ctx), Error = E> + Send + 'static> {
        let m = self.mapper.clone();

        let req = self.mapper.outgoing(Muxed::Request(req));
        Box::new(
            self.conn
                .request(ctx, req_id, target, req.req().unwrap())
                .map(move |(resp, ctx)| (m.incoming(Muxed::Response(resp)).resp().unwrap(), ctx) )
        )
    }

    fn respond(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, resp: MappedResp,
    ) -> Box<Future<Item = (), Error = E> + Send + 'static> {
        let resp = self.mapper.outgoing(Muxed::Response(resp));
        Box::new(self.conn.respond(ctx, req_id, target, resp.resp().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use crate::mapped::{Mapped, Mapper};
    use crate::mock::{MockConnector, MockTransaction};
    use crate::muxed::Muxed;

    use crate::connector::Connector;
    use futures::future::Future;

    #[derive(PartialEq, Debug, Clone)]
    struct A(u64);
    #[derive(PartialEq, Debug, Clone)]
    struct B(u64);

    #[derive(Clone)]
    struct MapImpl();

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
        let mut m = MockConnector::<u16, A, A, (), ()>::new();

        // Build wrapper
        let mut w =
            Mapped::<A, A, B, B, u64, u16, (), (), MockConnector<u16, A, A, (), ()>, MapImpl>::new(
                m.clone(),
                MapImpl(),
            );

        m.expect(vec![
            MockTransaction::request(1, A(0), Ok((A(1), ()))),
            MockTransaction::response(1, A(2), None),
        ]);

        let resp = w.request((), 0, 1, B(0)).wait().unwrap();
        assert_eq!(resp.0, B(1));

        w.respond((), 0, 1, B(2)).wait().unwrap();

        m.finalise();
    }
}

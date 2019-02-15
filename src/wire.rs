
use std::collections::HashMap;
use std::marker::PhantomData;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::clone::Clone;
use std::fmt::Debug;

use futures::future::{Future, ok, err};


use crate::connector::Connector;

pub struct Wire <ReqId, Target, Req, Resp, E, Ctx, H, I> {
    handler: H,
    instances: Arc<Mutex<HashMap<Target, I>>>,

    _req_id: PhantomData<ReqId>, 
    _req: PhantomData<Req>, 
    _resp: PhantomData<Resp>, 
    _e: PhantomData<E>, 
    _ctx: PhantomData<Ctx>,
}

impl <ReqId, Target, Req, Resp, E, Ctx, H, I> Clone for Wire<ReqId, Target, Req, Resp, E, Ctx, H, I>
where
    Target: Hash + PartialEq + Eq,
    H: FnMut(Req) -> Box<Future<Item=Resp, Error=E> + Sync + Send> + Clone + 'static,
{
    fn clone(&self) -> Self {
        Wire {
            handler: self.handler.clone(),
            instances: self.instances.clone(),

            _req_id: PhantomData,
            _req: PhantomData,
            _resp: PhantomData,
            _e: PhantomData,
            _ctx: PhantomData,
        }
    }
}

impl <ReqId, Target, Req, Resp, E, Ctx, H, I> Wire<ReqId, Target, Req, Resp, E, Ctx, H, I> 
where
    Target: Hash + PartialEq + Eq,
    I: Clone + Send + Sync + 'static,
    H: FnMut(I, Req) -> Box<Future<Item=Resp, Error=E>> + 'static,
{
    pub fn new(handler: H) -> Wire<ReqId, Target, Req, Resp, E, Ctx, H, I>  {
        Wire{
            handler,
            instances: Arc::new(Mutex::new(HashMap::new())),

            _req_id: PhantomData,
            _req: PhantomData,
            _resp: PhantomData,
            _e: PhantomData,
            _ctx: PhantomData,
        }
    }

    pub fn connector(&mut self, target: Target, instance: I) -> WireMux<ReqId, Target, Req, Resp, E, Ctx> {
        self.instances.lock().unwrap().insert(target, instance);

        WireMux::new()
    }
}

pub struct WireMux<ReqId, Target, Req, Resp, E, Ctx> {
    //connector: Wire<ReqId, Target, Req, Resp, E, Ctx>,

    _req_id: PhantomData<ReqId>, 
    _target: PhantomData<Target>,
    _req: PhantomData<Req>, 
    _resp: PhantomData<Resp>, 
    _e: PhantomData<E>, 
    _ctx: PhantomData<Ctx>,
}

impl <ReqId, Target, Req, Resp, E, Ctx> WireMux<ReqId, Target, Req, Resp, E, Ctx> {
    fn new() -> WireMux<ReqId, Target, Req, Resp, E, Ctx> {
        WireMux{
            _req_id: PhantomData,
            _target: PhantomData,
            _req: PhantomData,
            _resp: PhantomData,
            _e: PhantomData,
            _ctx: PhantomData,
        }
    }
}

impl <ReqId, Target, Req, Resp, E, Ctx> Connector<ReqId, Target, Req, Resp, E, Ctx> for WireMux <ReqId, Target, Req, Resp, E, Ctx> 
where
    ReqId: PartialEq + Debug + Send + 'static,
    Target: Hash + PartialEq + Eq + 'static,
    Req: PartialEq + Debug + Send + 'static,
    Resp: PartialEq + Debug + Send + 'static,
    E: PartialEq + Debug + Send + 'static,
    Ctx: Clone + PartialEq + Debug + Send + 'static,
    //H: FnMut(Req) -> Box<Future<Item=Resp, Error=E> + Sync + Send> + Clone + 'static,
{
        // Send a request and receive a response or error at some time in the future
    fn request(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, req: Req,
    ) -> Box<Future<Item = Resp, Error = E> + Send + 'static> {

        //let handlers = self.connector.handlers.lock().unwrap();
        //let mut handler = handlers.get(&target).unwrap().clone();

        //Box::new(handler(req))

        unimplemented!();
    }

    // Send a response message
    fn respond(
        &mut self, ctx: Ctx, req_id: ReqId, target: Target, resp: Resp,
    ) -> Box<Future<Item = (), Error = E> + Send + 'static> {
        unimplemented!();
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_Wire() {
        let mut i: Wire<u16, u64, u32, u32, (), (), _, _> = Wire::new(|inst, req| {
            Box::new(ok(req+1))
        });

        i.connector(1, 2u8);

    }

}
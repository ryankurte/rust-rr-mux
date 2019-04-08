/// Muxed is a container for either a request or response message
#[derive(Debug, Clone, PartialEq)]
pub enum Muxed<Req, Resp> {
    Request(Req),
    Response(Resp),
}

impl<Req, Resp> Muxed<Req, Resp> {
    /// Fetch a request if muxed contains a request type
    pub fn req(self) -> Option<Req> {
        match self {
            Muxed::Request(req) => Some(req),
            _ => None,
        }
    }

    /// Fetch a response if muxed contains a response type
    pub fn resp(self) -> Option<Resp> {
        match self {
            Muxed::Response(resp) => Some(resp),
            _ => None,
        }
    }
}

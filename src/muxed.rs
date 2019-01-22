

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

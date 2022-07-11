use serde::Deserialize;
use serde::Serialize;

use crate::models::peer::Peer;
use crate::requests::list_request::PeerListMode;

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
    pub mode: PeerListMode,
    pub data: Peer,
    pub receiver: String,
}

pub enum ListEventType {
    Response(ListResponse),
}

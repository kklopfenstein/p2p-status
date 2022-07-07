use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub hostname: String,
    pub description: String
}

use serde::Serialize;
use serde::Deserialize;

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageRequest {
    pub message: String,
    pub hostname: String
}
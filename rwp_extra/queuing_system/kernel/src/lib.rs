use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Payload {
    pub memory: Vec<u8>,
    pub contract: Vec<u8>,
}


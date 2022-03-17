pub mod rest;
mod sign_producer;
mod signed_topic_consumer;
mod worker;

use uuid::Uuid;

pub use worker::{SignRequester, Worker};

pub struct MsgToSign {
    // headers
    msg_id: String, //this is general id could be topic+partition_id+offset?
    resp_topic: String,
    // payload
    msg: String,
}

impl MsgToSign {
    pub fn new(msg: String, resp_topic: String) -> Self {
        Self {
            msg_id: Uuid::new_v4().to_string(),
            resp_topic,
            msg,
        }
    }

    pub fn msg_id(&self) -> &str {
        &self.msg_id
    }

    pub fn resp_topic(&self) -> &str {
        &self.resp_topic
    }

    pub fn msg(&self) -> &str {
        &self.msg
    }
}

pub struct MsgSigned {
    // headers
    msg_id: String, //this is general id could be topic+partition_id+offset?
    _resp_id: String,
    // payload
    signed_msg: String,
}
impl MsgSigned {
    pub fn new(req_msg_id: String, resp_msg_id: String, signed_msg: String) -> Self {
        Self {
            msg_id: req_msg_id,
            _resp_id: resp_msg_id,
            signed_msg,
        }
    }

    pub fn msg_id(&self) -> &str {
        &self.msg_id
    }

    pub fn signed_msg(&self) -> &str {
        &self.signed_msg
    }
}

use anyhow::bail;
use rdkafka::config::RDKafkaLogLevel;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::{BorrowedMessage, Headers, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::{ClientConfig, Message};
use std::env;
use std::time::Duration;
use tokio_stream::StreamExt;

// Use Jemalloc only for musl-64 bits platforms
#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

struct MsgToSign {
    // headers
    pub msg_id: String, //this is general id could be topic+partition_id+offset?
    pub resp_topic: String,
    // payload
    pub msg: String,
}

impl TryFrom<BorrowedMessage<'_>> for MsgToSign {
    type Error = anyhow::Error;

    fn try_from(value: BorrowedMessage) -> Result<Self, Self::Error> {
        let headers = value
            .headers()
            .ok_or_else(|| anyhow::Error::msg("expected headers in msg"))?;

        let (key, msg_id) = headers
            .get_as(0)
            .ok_or_else(|| anyhow::Error::msg("expected headers to not be empty"))?;
        if key != "msg_id" {
            bail!("expected `msg_id` header key at index 0 but found: {}", key)
        }
        let msg_id: &str = msg_id?;

        let (key, resp_topic) = headers
            .get_as(1)
            .ok_or_else(|| anyhow::Error::msg("expected headers to contains index 1"))?;
        if key != "resp_topic" {
            bail!(
                "expected `resp_topic` header key at index 1 but found {}",
                key
            )
        }
        let resp_topic: &str = resp_topic?;

        let msg = value
            .payload()
            .ok_or_else(|| anyhow::Error::msg("no payload"))?;
        let msg = String::from_utf8(msg.to_vec())?;

        Ok(Self {
            msg_id: msg_id.to_string(),
            resp_topic: resp_topic.to_string(),
            msg,
        })
    }
}

struct MsgSigned {
    // headers
    msg_id: String, //this is general id could be topic+partition_id+offset?
    resp_id: String,
    // payload
    signed_msg: String,
}

impl MsgSigned {
    fn from_unsigned(msg_to_sign: MsgToSign) -> MsgSigned {
        Self {
            msg_id: msg_to_sign.msg_id,
            resp_id: uuid::Uuid::new_v4().to_string(),
            signed_msg: base64::encode(msg_to_sign.msg),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let group_id =
        env::var("SIGNER_SERVICE_GROUP_ID").unwrap_or_else(|_| "signer.v1.service".to_string());
    let brokers =
        env::var("SIGNER_SERVICE_KAFKA_BROKERS").unwrap_or_else(|_| "127.0.0.1:9092".to_string());
    let req_topic =
        env::var("SIGNER_SERVICE_REQ_TOPIC").unwrap_or_else(|_| "signer.v1".to_string());

    tracing::debug!(
        r#"SIGNER_SERVICE_KAFKA_BROKERS: {}
SIGNER_SERVICE_GROUP_ID: {}
SIGNER_SERVICE_REQ_TOPIC: {}
"#,
        brokers,
        group_id,
        req_topic
    );

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("message.timeout.ms", "5000")
        .set_log_level(RDKafkaLogLevel::Debug)
        .create()?;

    let consumer: StreamConsumer = ClientConfig::new()
        .set("group.id", &group_id)
        .set("bootstrap.servers", &brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .set_log_level(RDKafkaLogLevel::Debug)
        .create()?;

    consumer.subscribe(&[&req_topic])?;

    let mut stream = consumer.stream();

    while let Some(req) = stream.next().await {
        tracing::trace!("recived req {:?}", req);
        let req = req?;

        let msg_to_sign = match MsgToSign::try_from(req) {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("unexpected format of request: {:?}", err);
                continue;
            }
        };

        let resp_topic = msg_to_sign.resp_topic.clone();
        let signed = MsgSigned::from_unsigned(msg_to_sign);

        let record = FutureRecord::<str, _>::to(&resp_topic)
            .payload(&signed.signed_msg)
            .headers(
                OwnedHeaders::new()
                    .add("msg_id", &signed.msg_id)
                    .add("resp_id", &signed.resp_id),
            );

        producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(err, _ow_msg)| err)?;
    }

    Ok(())
}

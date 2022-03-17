//! Heart of dealing with kafka signing

use rdkafka::{
    config::RDKafkaLogLevel,
    consumer::{Consumer, StreamConsumer},
    error::KafkaError,
    ClientConfig,
};
use std::collections::HashMap;
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver, Sender},
        oneshot,
    },
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::signed_topic_consumer::TopicConsumeErr;
use crate::{sign_producer::SignProducer, signed_topic_consumer::new_signed_topic_consumer};
use crate::{MsgSigned, MsgToSign};

type SignPromiseItem = Result<MsgSigned, TopicConsumeErr>;
type SignPromiseTx = oneshot::Sender<SignPromiseItem>;
/// Promise that in some in futre we will receive signed message or error
type SignPromiseRx = oneshot::Receiver<SignPromiseItem>;

#[derive(Debug, Clone)]
pub struct SignRequester {
    resp_topic: String,
    inner: Sender<(MsgToSign, SignPromiseTx)>,
}

impl SignRequester {
    pub async fn start_req(&self, msg: String) -> Result<SignPromiseRx, ()> {
        let req = MsgToSign::new(msg, self.resp_topic.clone());
        let (tx, rx) = oneshot::channel();
        self.inner.send((req, tx)).await.map_err(drop)?;
        Ok(rx)
    }
}

pub struct Worker {
    request_stream: ReceiverStream<(MsgToSign, SignPromiseTx)>,
    consumer: StreamConsumer,
    waiting_reqs: HashMap<String, SignPromiseTx>,
}

impl Worker {
    /// Spawn worker and return a `SignRequester` to create sign requests
    ///
    /// `req_topic` -- is producer topic. One topic for as many application as you wish
    /// `resp_topic` -- consumer. must be used only by one instance of application
    pub fn spawn_new(
        req_topic: &str,
        resp_topic: &str,
        brokers: &str,
    ) -> Result<SignRequester, KafkaError> {
        let (loopback_err_tx, loopback_err_rx) = mpsc::channel(1024);

        let producer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()?;

        let (sing_producer, sender) = SignProducer::new(req_topic, producer, loopback_err_tx);

        let (req_tx, req_rx) = mpsc::channel(1024);

        let consumer: StreamConsumer = ClientConfig::new()
            .set("group.id", "test.group.id")
            .set("bootstrap.servers", brokers)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            .set_log_level(RDKafkaLogLevel::Debug)
            .create()?;

        consumer.subscribe(&[resp_topic])?;

        let worker = Self {
            request_stream: ReceiverStream::new(req_rx),
            consumer,
            waiting_reqs: HashMap::with_capacity(2048),
        };

        //spawn producer and consumer tasks
        // TODO: handle error of workers

        tokio::spawn(async move {
            worker
                .work(loopback_err_rx, sender)
                .await
                .expect("expect to work");
        });

        tokio::spawn(async move {
            sing_producer.worker().await;
        });

        Ok(SignRequester {
            inner: req_tx,
            resp_topic: resp_topic.to_string(),
        })
    }

    async fn work(
        mut self,
        sending_err: Receiver<TopicConsumeErr>,
        producer: Sender<MsgToSign>,
    ) -> Result<(), ()> {
        let consumer_stream = self.consumer.stream();

        let mut singed_msgs =
            new_signed_topic_consumer(consumer_stream, ReceiverStream::new(sending_err));

        loop {
            select! {
                new_req = self.request_stream.next() => match new_req {
                    Some((msg_req, here_resp_will_be_send_when_ready)) => {

                        let msg_id = msg_req.msg_id().to_string();
                        match producer.send(msg_req).await {
                            Ok(_) => (),
                            Err(_err) => todo!("sing producer dropped"), // TODO: handle droping sing_producer
                        }

                        // wait for response
                        let old = self.waiting_reqs.insert(msg_id, here_resp_will_be_send_when_ready);
                        assert!(old.is_none());
                    },
                    None => todo!(), // TODO: no more request will be provided we should run some teardown function
                },
                new_res = singed_msgs.next() => match new_res {
                    Some(resp) => {
                        send_resp(&mut self.waiting_reqs, resp)
                    },
                    None => todo!(), // TODO: consumer disconnected. This could be probably rerunned
                }
            }
        }
    }
}

fn send_resp(
    waiting_reqs: &mut HashMap<String, SignPromiseTx>,
    resp: Result<MsgSigned, TopicConsumeErr>,
) {
    match resp {
        Ok(resp) => {
            let msg_id = resp.msg_id().to_owned();
            send_resp_impl(waiting_reqs, &msg_id, Ok(resp))
        }
        Err(err) => {
            if let Some(msg_id) = err.msg_id() {
                let msg_id = msg_id.to_owned();
                send_resp_impl(waiting_reqs, &msg_id, Err(err))
            }
        }
    }
}

fn send_resp_impl(
    waiting_reqs: &mut HashMap<String, SignPromiseTx>,
    msg_id: &str,
    resp: Result<MsgSigned, TopicConsumeErr>,
) {
    if let Some(tx) = waiting_reqs.remove(msg_id) {
        if let Err(_nobody_wanted_resp) = tx.send(resp) {
            //TODO: rx was dropped nobody want our response
        }
    } else {
        //TODO: log dropeped response
    }
}

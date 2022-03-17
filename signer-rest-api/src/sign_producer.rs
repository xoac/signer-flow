use std::time::Duration;

use futures::StreamExt;
use rdkafka::{
    message::{Headers, OwnedHeaders},
    producer::{FutureProducer, FutureRecord},
    Message,
};
use tokio::sync::mpsc::{self, Sender};
use tokio_stream::wrappers::ReceiverStream;

use crate::{signed_topic_consumer::TopicConsumeErr, MsgToSign};

pub struct SignProducer {
    topic: String,
    timeout: Duration,

    requests: ReceiverStream<MsgToSign>,
    inner: FutureProducer,
    sending_err: Sender<TopicConsumeErr>,
}

impl SignProducer {
    pub fn new(
        topic: impl Into<String>,
        producer: FutureProducer,
        sending_err: Sender<TopicConsumeErr>,
    ) -> (SignProducer, Sender<MsgToSign>) {
        let (tx, rx) = mpsc::channel(1024);
        (
            Self {
                topic: topic.into(),
                timeout: Duration::from_secs(5),
                requests: ReceiverStream::new(rx),
                inner: producer,
                sending_err,
            },
            tx,
        )
    }

    pub async fn worker(mut self) {
        while let Some(req) = self.requests.next().await {
            let topic = self.topic.clone();
            let se = self.sending_err.clone();
            let producer = self.inner.clone();

            //TODO: Instead of spawning new task for each future we could use futures::FutresUnordered
            tokio::spawn(async move {
                let record = FutureRecord::<str, str>::to(&topic)
                    .headers(
                        OwnedHeaders::new_with_capacity(2)
                            .add("msg_id", req.msg_id())
                            .add("resp_topic", req.resp_topic()),
                    )
                    .payload(req.msg());

                let f = producer.send(record, self.timeout);

                let r = f.await;
                match r {
                    Ok(_) => (),
                    Err((err, msg)) => {
                        let (key, value) = msg
                            .headers()
                            .unwrap()
                            .get_as(0)
                            .expect("msg_id should be present");
                        assert_eq!(key, "msg_id");

                        let value: &str = value.expect("value should be String");

                        se.send(TopicConsumeErr::new(Some(value.to_string()), err))
                            .await
                            .expect("expected loopback err")
                    }
                }
            });
        }
    }
}

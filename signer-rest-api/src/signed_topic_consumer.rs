use rdkafka::message::Headers;
use rdkafka::Message;
use tokio_stream::wrappers::ReceiverStream;

use crate::MsgSigned;
use futures::ready;
use rdkafka::{consumer::MessageStream, error::KafkaError};

use tokio_stream::Stream;

use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

#[derive(Debug, Clone, thiserror::Error)]
#[error("failed to receive data from consumer")]
pub struct TopicConsumeErr {
    msg_id: Option<String>,
    #[source]
    source_err: KafkaError,
}
impl TopicConsumeErr {
    pub fn new(msg_id: Option<String>, source_err: KafkaError) -> Self {
        Self { msg_id, source_err }
    }

    /// This error has attached msg_id
    pub fn msg_id(&self) -> Option<&str> {
        self.msg_id.as_deref()
    }
}

impl From<KafkaError> for TopicConsumeErr {
    fn from(err: KafkaError) -> Self {
        Self::new(None, err)
    }
}

pin_project! {
    pub struct SignedTopicConsumer<'a> {
        //TODO: implement alternates between streams similar to tokio_stream::Merge
        _consumer_first: bool,
        // waiting for responses from kafka
        #[pin]
        consumer: MessageStream<'a>,
        // this stream will contain error if SingProducer will fail to send item to topic
        #[pin]
        sending_err: ReceiverStream<TopicConsumeErr>,
    }
}

pub fn new_signed_topic_consumer(
    consumer: MessageStream<'_>,
    sending_err: ReceiverStream<TopicConsumeErr>,
) -> SignedTopicConsumer<'_> {
    SignedTopicConsumer {
        _consumer_first: false,
        consumer,
        sending_err,
    }
}

impl<'a> Stream for SignedTopicConsumer<'a> {
    type Item = Result<MsgSigned, TopicConsumeErr>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let me = self.project();

        // TODO implement Fuse and done for both streams

        if let Poll::Ready(item) = me.consumer.poll_next(cx) {
            match item {
                Some(Ok(raw_msg)) => {
                    let signed_msg = raw_msg
                        .payload()
                        .expect("we assume that response from kafka conains correct format");
                    let signed_msg =
                        String::from_utf8(signed_msg.to_owned()).expect("assume String is correct");

                    let headers = raw_msg.headers().expect("expect headers");
                    let (key, req_msg_id) = headers.get_as(0).expect("msg_id");
                    assert_eq!(key, "msg_id");
                    let req_msg_id: &str = req_msg_id.unwrap();

                    let (key, resp_msg_id) = headers.get_as(1).expect("resp_id");
                    assert_eq!(key, "resp_id");
                    let resp_msg_id: &str = resp_msg_id.unwrap();

                    let singed_msg =
                        MsgSigned::new(req_msg_id.to_string(), resp_msg_id.to_string(), signed_msg);

                    return Poll::Ready(Some(Ok(singed_msg)));
                }
                Some(Err(err)) => return Poll::Ready(Some(Err(TopicConsumeErr::from(err)))),
                None => todo!(),
            }
        };

        let possible_err = ready!(me.sending_err.poll_next(cx));
        match possible_err {
            Some(err) => Poll::Ready(Some(Err(err))),
            None => todo!(),
        }
    }
}

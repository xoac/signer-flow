use signer_rest_api::Worker;
use std::env;

// Use Jemalloc only for musl-64 bits platforms
#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let brokers =
        env::var("SIGNER_REST_API_KAFKA_BROKERS").unwrap_or_else(|_| "127.0.0.1:9092".to_string());
    let req_topic =
        env::var("SIGNER_REST_API_REQ_TOPIC").unwrap_or_else(|_| "signer.v1".to_string());
    let res_topic = env::var("SIGNER_REST_API_RES_TOPIC")?; //This is required

    tracing::info!("SIGNER_REST_API_KAFKA_BROKERS: {}", brokers);
    tracing::trace!("trace level enabled");

    let sign_reqester = Worker::spawn_new(&req_topic, &res_topic, &brokers)?;

    let router = signer_rest_api::rest::router(sign_reqester);

    axum::Server::bind(&"0.0.0.0:80".parse().unwrap())
        .serve(router.into_make_service())
        .await?;

    Ok(())
}

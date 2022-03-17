//! Represent REST API

use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        WebSocketUpgrade,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

use tower_http::trace::TraceLayer;

use crate::worker::SignRequester;

pub fn router(requester: SignRequester) -> Router {
    Router::new()
        .route("/sign", get(sign_index))
        .route(
            "/sign/ws",
            get(move |ws| ws_handler(ws, Clone::clone(&requester))),
        )
        .layer(TraceLayer::new_for_http())
}

async fn sign_index() -> Html<String> {
    let fronted = r#"
    <!DOCTYPE html>
<html lang="en">
<body>
<!-- message form -->
<form name="publish">
  <input type="text" name="message">
  <input type="submit" value="Send">
</form>

<!-- div with messages -->
<div id="messages"></div>
  <script>
  let socket = new WebSocket("ws://" + location.host + "/sign/ws");
// send message from the form
document.forms.publish.onsubmit = function() {
  let outgoingMessage = this.message.value;

  socket.send(outgoingMessage);
  return false;
};

// message received - show the message in div#messages
socket.onmessage = function(event) {
  let message = event.data;

  let messageElem = document.createElement('div');
  messageElem.textContent = message;
  document.getElementById('messages').prepend(messageElem);
}
  </script>
</body>
</html>
"#
    .to_string();

    Html::from(fronted)
}

async fn ws_handler(ws: WebSocketUpgrade, requester: SignRequester) -> impl IntoResponse {
    ws.on_upgrade(|socket| singn_ws_kafka_handler(socket, requester))
}

async fn singn_ws_kafka_handler(mut socket: WebSocket, requester: SignRequester) {
    while let Some(msg) = socket.recv().await {
        let promise_sign_msg = if let Ok(msg) = msg {
            match msg {
                Message::Text(t) => {
                    println!("client send msg to sign: {:?}", t);
                    match requester.start_req(t).await {
                        Ok(promise_sign_msg) => promise_sign_msg,
                        Err(_) => todo!(), // server error
                    }
                }
                Message::Binary(_) => {
                    todo!()
                }
                Message::Ping(_) => continue,
                Message::Pong(_) => continue,
                Message::Close(_) => {
                    println!("client disconnected");
                    return;
                }
            }
        } else {
            //"client disconnected"
            return;
        };

        // prepare response
        //TODO: SingRequester should be able to handle all this error internally
        let text_to_send = async move {
            // handle timeout
            let fut_res = match tokio::time::timeout(Duration::from_secs(5), promise_sign_msg).await
            {
                Ok(fut_res) => fut_res,
                Err(_elapsed) => return "error: timeout".to_string(),
            };

            // resolve receiver (promise_sign_msg)
            let fut_res = match fut_res {
                Ok(fut_res) => fut_res,
                Err(_recv_err) => return "error: internal error".to_string(),
            };

            match fut_res {
                Ok(signed_msg) => {
                    format!("ok: {}", signed_msg.signed_msg())
                }
                Err(err) => {
                    format!("error: {:?}", err)
                }
            }
        }
        .await;

        match socket.send(Message::Text(text_to_send)).await {
            Ok(_) => (),
            Err(_disconnected) => return,
        }
    }
}

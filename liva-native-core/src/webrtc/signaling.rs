use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

#[derive(Serialize, Deserialize, Debug)]
pub struct SignalingMessage {
    pub sdp: String,
    #[serde(rename = "type")]
    pub msg_type: String, // "offer" or "answer"
}

pub struct SignalingServer {
    pub port: u16,
}

impl SignalingServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        println!("WebRTC Signaling server listening on ws://{}", addr);

        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(handle_connection(stream));
        }
        
        Ok(())
    }
}

async fn handle_connection(stream: TcpStream) {
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("Error during the websocket handshake: {}", e);
            return;
        }
    };

    let (_, mut receiver) = ws_stream.split();
    
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                println!("Received WS message: {}", text);
                if let Ok(sig_msg) = serde_json::from_str::<SignalingMessage>(&text) {
                    println!("Parsed SDP {}: \n{}", sig_msg.msg_type, sig_msg.sdp);
                    // TODO: Pass to WebRTC PeerConnection and reply with Answer
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}

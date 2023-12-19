use anyhow::bail;
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;
use futures::{SinkExt, StreamExt};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::tungstenite;
use serde::Deserialize;
use anyhow::anyhow;
use ethers::types::{Address, H256, Bytes};

pub struct EventReport {
    pub txhash: H256,
    pub recipient: Address
}

const WEBSOCKET_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(60*10);

#[derive(Deserialize,Debug, Clone)]
pub struct EthSubscription {
    pub params: EthSubscriptionParams
}

#[allow(non_snake_case)]
#[derive(Deserialize,Debug, Clone)]
pub struct EthSubscriptionResults {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Bytes,
    pub transactionHash: H256
}

#[derive(Deserialize,Debug, Clone)]
pub struct EthSubscriptionParams {
    pub result: EthSubscriptionResults
}

pub struct EventSubscriptions {
    pub ws_server: String,
    pub contract: Address,
    sender: async_channel::Sender<EventReport>
}

#[derive(Deserialize,Debug, Clone)]
struct SubscribeId {
    #[serde(rename = "result")]
    pub subscription_id: String
}

impl EventSubscriptions {


    pub fn new(ws_server: String, sender: async_channel::Sender<EventReport>, contract: Address) -> Self {
        Self {
            ws_server,
            sender,
            contract
        }
    }

    pub async fn run(&mut self) {
        self.get_events().await;
    }

    // TokensBridged(address indexed token, address indexed recipient, uint256 value, bytes32 indexed messageId)
    fn get_subscription_command(&self) -> &str {
        r#"{"jsonrpc":"2.0","id":1,"method":"eth_subscribe","params": ["logs", {"topics": ["0x9afd47907e25028cdaca89d193518c302bbb128617d5a992c5abd45815526593"]}]}"#
    }

    async fn get_events(&mut self) {
        let subscribe_command = self.get_subscription_command().to_string();

        'outer: loop {
            match self.connect_and_subscribe(&subscribe_command).await {
                Ok(mut ws_stream) => {
                    // Poll
                    loop {
                        if let Err(err) = self.poll_and_handle_events(&mut ws_stream).await {
                            println!("Event watcher error... reconnecting: {}", err);
                            sleep(Duration::from_secs(2)).await;
                            continue 'outer;
                        }
                    }
                }
                Err(err) => {
                    println!("Event watcher error... reconnecting: {}", err);
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    async fn connect_and_subscribe(&mut self, subscribe_command: &str) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, tungstenite::Error> {

        let (mut ws_stream, _) = connect_async(&self.ws_server).await?;

        // subscribe
        ws_stream.send(subscribe_command.into()).await?;
        println!("subscription message sent");


        let msg = ws_stream.next().await.ok_or_else(|| tungstenite::Error::Io(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "blank response to subscription")))?;
        if let tungstenite::Message::Text(text) = msg? {

            let response: Result<SubscribeId, serde_json::Error> = serde_json::from_str(&text);
            if let Ok(subdata) = response {
                println!("Received subscription id: {}", subdata.subscription_id);
            }   
        }

        Ok(ws_stream)
    }

    async fn poll_and_handle_events(&mut self, ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> anyhow::Result<()> {

        let result = tokio::time::timeout(WEBSOCKET_TIMEOUT, ws_stream.next()).await;

        let msg = match result {
            Ok(_option) => {
                _option.ok_or_else(|| anyhow!("blank response to poll"))?
            },
            Err(_) => {
                bail!("Websocket timeout");
            },
        };

        if let tungstenite::Message::Text(text) = msg? {
            let s = serde_json::from_str::<EthSubscription>(&text)?;


            if s.params.result.address == self.contract {
                if let Some(recipient_hash) = s.params.result.topics.get(2) {
                    if let Some(byte_slice) = recipient_hash.0.get(12..32) {
                        let recipient = Address::from_slice(byte_slice);
    
                        let report = EventReport { 
                            txhash: s.params.result.transactionHash,
                            recipient
                         };
    
                        if let Err(err) = self.sender.try_send(report) {
                            println!("Report sending failed: {}",err);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
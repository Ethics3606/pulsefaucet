use crate::config::Config;
use anyhow::bail;
use ethers::prelude::Provider;
use ethers::prelude::Ws;
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::H256;
use ethers::utils::keccak256;
use ethers::signers::Signer;
use tokio::time::sleep;
use std::time::Duration;
use ethers::types::Address;
use ethers::prelude::Middleware;
use ethers::prelude::Wallet;
use std::sync::Arc;
use fxhash::FxHashSet;
use crate::event::EventSubscriptions;
use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::types::U256;
use crate::event::EventReport;
use ethers::types::TransactionRequest;
use tokio::time;
use tokio::time::Instant;

pub struct FaucetBot {
    pub config: Config,
    pub wallet: Wallet<SigningKey>,
    pub conn: Option<Arc<Provider<Ws>>>,
    pub nonce: U256,
    pub last_gift_time: Option<Instant>
}

impl FaucetBot {
    pub fn new(config: Config, wallet: Wallet<SigningKey>) -> Self {
        Self {
            config,
            wallet,
            conn: None,
            nonce: U256::zero(),
            last_gift_time: None
        }
    }

    pub async fn update_nonce(&mut self) -> anyhow::Result<()> {

        if let Some(conn) = &self.conn {
            match conn.get_transaction_count(self.wallet.address(), None).await {
                Ok(nonce) => {
                    println!("Setting nonce to {}",nonce);
                    self.nonce = nonce;
                },
                Err(err) => {
                    bail!(format!("Get nonce error: {}",err));
                }
            }
        }

        Ok(())
    }

    pub async fn run_update_nonce(&mut self) {
        while self.update_nonce().await.is_err() {
            sleep(Duration::from_secs(2)).await; // Incase of unexpected scenario
            self.connect_ws().await;
        }
    }

    pub async fn run(&mut self) {

        self.connect_ws().await;

        self.run_update_nonce().await;


        let (event_sender, event_receiver) = async_channel::unbounded::<EventReport>();
        let mut event_subscription = EventSubscriptions::new(self.config.server_settings.ws_server.clone(),event_sender,self.config.bridge_settings.contract);

        tokio::spawn(async move {
            event_subscription.run().await;
        });


        let mut gift_history = FxHashSet::<Address>::default();

        let mut interval_nonce_check = time::interval(time::Duration::from_secs(60*2));
        interval_nonce_check.tick().await;

        loop {
            tokio::select! {

                result = event_receiver.recv() => {
                    match result {
                        Ok(report) => {
                            println!("New Bridge event: {:?} Recipient: {:?}",report.txhash, report.recipient);

                            if gift_history.insert(report.recipient) {
                                if let Err(err) = self.run_sequence(report.recipient).await {
                                    println!("Gift giving error!: {}",err);
                                }
                            }
                        },
                        Err(err) => {
                            println!("Event channel error: {}",err);
                            std::process::exit(1);
                        }
                    }
                },
                _ = interval_nonce_check.tick() => { // This will ping and keep the websocket connection alive

                    if let Some(t0) = self.last_gift_time {
                        let t1 = Instant::now();

                        let dt = t1 - t0;

                        if dt.as_secs() > 60 {
                            self.run_update_nonce().await;
                        }
                    }
                }

            }
        }

    }

    pub async fn run_sequence(&mut self, recipient: Address) -> anyhow::Result<()> {

        let gift_amount = U256::from(self.config.faucet_settings.gift_amount) * U256::exp10(18);

        if let Some(conn) = &self.conn {

            let bot_balance = conn.get_balance(self.wallet.address(), None).await?;

            if bot_balance < gift_amount {
                bail!("Insufficient balance to give a gift");
            }

            let recipient_balance = conn.get_balance(recipient, None).await?;

            if recipient_balance >= gift_amount {
                println!("The user has sufficient gas");
                return Ok(());
            }

            let actual_gift = gift_amount - recipient_balance;

            let gas_price = conn.get_gas_price().await?;

            let tx = TransactionRequest::new()
                .chain_id(self.config.server_settings.chain_id)
                .gas(25000)
                .value(actual_gift)
                .to(recipient)
                .gas_price(gas_price)
                .nonce(self.nonce);

            let tx_typed = TypedTransaction::Legacy(tx);
            let tx_signature = self.wallet.sign_transaction(&tx_typed).await?;
            let tx_bytes = tx_typed.rlp_signed(&tx_signature);
            let txhash: H256 = keccak256(tx_bytes.clone()).into();

            match conn.send_raw_transaction(tx_bytes).await {
                Ok(_) => {
                    println!("Gift transaction sent: {:?}",txhash);
                    self.nonce += U256::one();
                    self.last_gift_time = Some(Instant::now());
                },
                Err(err) => {
                    println!("Sent tx error: {}",err);
                }
            }
        }
        Ok(())
    }

    pub async fn connect_ws(&mut self) {

        'outer: loop {
            let result = Provider::<Ws>::connect_with_reconnects(&self.config.server_settings.ws_server,999999999).await;

            match result {
                Ok(c) => {
                    self.conn = Some(Arc::new(c));
                    break 'outer;
                },
                Err(err) => {
                    println!("Connection error: {}", err);
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
        println!("Connection established to ws server: {}",self.config.server_settings.ws_server);
    }
}
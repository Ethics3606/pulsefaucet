pub mod event;
pub mod config;
pub mod bot;

use crate::config::Config;
use std::env;
use ethers::prelude::LocalWallet;
use ethers::signers::Signer;
use crate::bot::FaucetBot;



#[tokio::main(flavor = "multi_thread")]
async fn main() {

    let config = Config::new().unwrap();


    let pk_hex = match env::var("PrivateKey") {
        Ok(key) => key,
        Err(_) => {
            println!("Private key is missing. Set it in your environmental variables under the name PrivateKey");
            std::process::exit(1);
        }
    };

    let wallet = pk_hex.parse::<LocalWallet>().unwrap().with_chain_id(config.server_settings.chain_id);

    let mut faucet_bot = FaucetBot::new(config,wallet);

    faucet_bot.run().await;
}


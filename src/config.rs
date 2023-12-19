use serde::Deserialize;
use std::fs;
use ethers::types::Address;



#[derive(Deserialize,Debug, Clone)]
pub struct ServerSettings {
    pub ws_server: String,
    pub chain_id: u64
}


#[derive(Deserialize,Debug, Clone)]
pub struct BridgeSettings {
    pub contract: Address
}

#[derive(Deserialize,Debug, Clone)]
pub struct FaucetSettings {
    pub gift_amount: u64
}

#[derive(Deserialize,Debug, Clone)]
pub struct Config {
    pub server_settings: ServerSettings,
    pub bridge_settings: BridgeSettings,
    pub faucet_settings: FaucetSettings,
}

impl Config {

    pub fn new() -> anyhow::Result<Config> {
        let toml_string = fs::read_to_string("config/config.toml").unwrap();
        let res: Result<Config,toml::de::Error> = toml::from_str(&toml_string);
        Ok(res?)
    }
}
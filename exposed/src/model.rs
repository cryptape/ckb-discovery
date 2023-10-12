use serde::{Deserialize, Serialize};
use std::time::SystemTime;
// Define a struct to represent a peer with country and city information
#[derive(Clone, Serialize, Deserialize)]
pub struct Peer {
    pub(crate) id: i32,
    //pub(crate) ip: String,
    pub(crate) version: String,
    pub(crate) version_short: String,
    pub(crate) last_seen: Option<SystemTime>,
    pub(crate) country: Option<String>,
    pub(crate) city: Option<String>,
    //pub(crate) address: String,
    pub(crate) latitude: Option<f64>,
    pub(crate) longitude: Option<f64>,
    pub(crate) node_type: i32, // use i32 here cuz orm limitation
}

#[derive(Serialize, Deserialize)]
pub struct ServiceStatus {
    pub(crate) last_update: u64,
}


#[derive(Deserialize, Default)]
pub struct QueryParams {
    #[serde(default = "default_network")]
    pub(crate) network: String,
    #[serde(default = "default_timeout")]
    pub(crate) offline_timeout: u64,
    #[serde(default = "default_unknown_timeout")]
    pub(crate) unknown_offline_timeout: u64,
}

fn default_network() -> String {
    "mirana".to_string()
}

fn default_timeout() -> u64 {
    std::env::var("MARCI_DEFAULT_TIMEOUT")
        .unwrap_or("43200".to_string())
        .parse()
        .unwrap()
}

fn default_unknown_timeout() -> u64 {
    std::env::var("MARCI_DEFAULT_UNKNOWN_TIMEOUT")
        .unwrap_or("1440".to_string())
        .parse()
        .unwrap()
}
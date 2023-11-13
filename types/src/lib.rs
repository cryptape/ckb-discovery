use std::net::IpAddr;
use serde::{Serialize, Deserialize};

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum CKBNetworkType {
    Mirana,
    Pudge,
    Dev
}

impl From<String> for CKBNetworkType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "mirana"|"ckb"|"main" => CKBNetworkType::Mirana,
            "pudge"|"ckb_testnet"|"test" => CKBNetworkType::Pudge,
            "dev"|"ckb_dev" => CKBNetworkType::Dev,
            _ => CKBNetworkType::Mirana,
        }
    }
}

impl CKBNetworkType {
    pub fn into_legacy_str(&self) -> String {
        match self {
            CKBNetworkType::Mirana => "ckb".to_string(),
            CKBNetworkType::Pudge => "ckb_testnet".to_string(),
            CKBNetworkType::Dev => "ckb_dev".to_string(),
        }
    }

    pub fn into_str(&self) -> String {
        match self {
            CKBNetworkType::Mirana => "mirana".to_string(),
            CKBNetworkType::Pudge => "pudge".to_string(),
            CKBNetworkType::Dev => "dev".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EndpointInfo {
    pub address: IpAddr,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Clone,  Debug)]
pub struct ReachableInfo {
    pub peer: NodeMetaInfo,
    pub from: NodeMetaInfo,
}

impl ReachableInfo {
    pub fn new(peer: NodeMetaInfo, from: NodeMetaInfo) -> Self {
        ReachableInfo {
            peer,
            from,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NodeMetaInfo {
    pub peer_id: String,
    pub addresses: Vec<EndpointInfo>,
    pub network: CKBNetworkType,
}


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerInfo {
    pub info: NodeMetaInfo,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub full: bool,
    #[serde(default)]
    pub is_ex: bool,
}

impl PeerInfo {
    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json_string: String) -> Result<Self, serde_json::error::Error>  {
        serde_json::from_str(json_string.as_str())
    }
}
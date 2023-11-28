extern crate core;

use paho_mqtt as mqtt;
use std::time::UNIX_EPOCH;
use redis::{AsyncCommands, Commands};
use tokio::time::{Duration, Instant, interval};
use ckb_discovery_types::{CKBNetworkType, EndpointInfo, NodeMetaInfo, PeerInfo, ReachableInfo};
use log::{debug, error, info, log, warn};
use std::{env};
use std::time::SystemTime;
use ipinfo::{IpDetails, IpError, IpInfo};
use lazy_static::lazy_static;
use std::sync::Mutex;
use std::collections::HashMap;
use futures::StreamExt;


async fn check_peer_online(con: &mut redis::aio::Connection, peer_id: String) -> bool {
    con.keys::<&String, Vec<String>>(&peer_id).await.unwrap_or_default().contains(&peer_id)
}

macro_rules! online_peer_key_format {
    () => ("peer.online.{}")
}

macro_rules! online2_peer_key_format {
    () => ("peer.online2.{}")
}

macro_rules! reachable_peer_key_format {
    () => ("peer.reachable.{}")
}

macro_rules! unknown_peer_key_format {
    () => ("peer.unknown.{}")
}

macro_rules! peer_seen_key_format {
    () => ("peer_info.{}.last_seen")
}

macro_rules! peer_ip_key_format {
    () => ("peer_info.{}.ip")
}

macro_rules! peer_version_key_format {
    () => ("peer_info.{}.version")
}

macro_rules! peer_country_key_format {
    () => ("peer_info.{}.country")
}

macro_rules! peer_city_key_format {
    () => ("peer_info.{}.city")
}

macro_rules! peer_region_key_format {
    () => ("peer_info.{}.region")
}


macro_rules! peer_pos_key_format {
    () => ("peer_info.{}.pos")
}

macro_rules! peer_witnesses_key_format {
    () => ("peer_info.{}.witnesses")
}

macro_rules! peer_network_key_format {
    () => ("peer_info.{}.network")
}

macro_rules! peer_network_quick_key_format {
    () => ("network.peer.{}.{}")
}

lazy_static! {
    static ref IPINFO: Mutex<IpInfo> = {
        let ipinfo_io_token = match ::std::env::var("IPINFO_IO_TOKEN") {
            Ok(token) if !token.is_empty() => Some(token),
            _ => {
                log::warn!("Miss environment variable \"IPINFO_IO_TOKEN\", use empty value");
                None
            }
        };
        let ipinfo = ipinfo::IpInfo::new(ipinfo::IpInfoConfig {
            token: ipinfo_io_token,
            cache_size: 10000,
            ..Default::default()
        })
        .expect("Connect to https://ipinfo.io");
        Mutex::new(ipinfo)
    };
    static ref IPINFO_CACHE: Mutex<HashMap<String, IpDetails>> = Mutex::new(Default::default());
}

pub async fn lookup_ipinfo(ip: &str) -> Result<IpDetails, IpError> {
    if let Ok(cache) = IPINFO_CACHE.lock() {
        if let Some(ipdetails) = cache.get(&ip.to_string()) {
            return Ok(ipdetails.clone());
        }
    }

    if let Ok(mut ipinfo) = IPINFO.lock() {
        let lookup_info = ipinfo.lookup(&ip).await;
        match lookup_info {
            Ok(ipdetails) => {
                if let Ok(mut cache) = IPINFO_CACHE.lock() {
                    cache.insert(ip.to_string(), ipdetails.to_owned());
                }

                return Ok(ipdetails.to_owned());
            }
            Err(err) => {
                warn!("IPINFO.lookup(\"{}\"), error: {}", ip, err);
                return Err(err)
            },
        }
    }

    unreachable!()
}

pub async fn query_by_reachable(reachable: &ReachableInfo) -> Option<IpDetails> {
    for addr in reachable.peer.addresses.iter() {
        if let Ok(res) = lookup_ipinfo(addr.address.to_string().as_str()).await {
            return Some(res)
        }
    }
    None
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env::set_var("RUST_LOG", "info");
    let ckb_node_default_timeout = env::var("CKB_NODE_DEFAULT_TIMEOUT").unwrap_or("5184000".to_string()).parse::<usize>()?;
    let ckb_node_ex_timeout = env::var("CKB_NODE_EX_DEFAULT_TIMEOUT").unwrap_or("7200".to_string()).parse::<usize>()?;
    let ckb_node_unknown_default_timeout = env::var("CKB_NODE_UNKNOWN_DEFAULT_TIMEOUT").unwrap_or("1209600".to_string()).parse::<usize>()?;
    let ckb_node_default_witnesses = env::var("CKB_NODE_DEFAULT_WITNESSES").unwrap_or("3".to_string()).parse::<usize>()?;

    let marci_broadcast_interval = env::var("MERCI_DEFAULT_INTERVAL").unwrap_or("180".to_string()).parse::<u64>()?;

    env_logger::init();
    let (online_tx, mut online_rx) = tokio::sync::mpsc::channel::<PeerInfo>(100);
    let (reachable_tx, mut reachable_rx) = tokio::sync::mpsc::channel::<ReachableInfo>(500);

    let (unknown_tx, mut unknown_rx) = tokio::sync::mpsc::channel::<NodeMetaInfo>(100);

    let mqtt_url = env::var("MQTT_URL").unwrap_or("mqtt:1883".to_string());
    let mqtt_user = env::var("MQTT_USER").unwrap_or("ckb".to_string());
    let mqtt_pass = env::var("MQTT_PASS").unwrap_or("ckbdiscovery".to_string());

    let create_ops = mqtt::CreateOptionsBuilder::new()
        .server_uri(mqtt_url.clone())
        .client_id("MARCI_v5")
        .max_buffered_messages(1000)
        .finalize();

    let mut mqtt_client = mqtt::AsyncClient::new(create_ops).expect("Failed to create MQTT client");

    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .clean_start(true)
        .keep_alive_interval(Duration::from_millis(100))
        .automatic_reconnect(Duration::from_millis(500), Duration::from_millis(2000))
        .user_name(mqtt_user)
        .password(mqtt_pass)
        .properties(mqtt::properties![mqtt::PropertyCode::SessionExpiryInterval => 5000])
        .finalize();

    mqtt_client.connect(conn_opts).await?;

    println!("Connected to MQTT!");

    let mut mqtt_con = mqtt_client.get_stream(None);

    let sub_context = mqtt_client.clone().to_owned();
    let pub_context = mqtt_client.clone().to_owned();

    let topics = vec!["peer/online", "peer/reachable", "peer/unknown", "peer/online2"];
    let qos = vec![mqtt::QOS_1,mqtt::QOS_1,mqtt::QOS_1,mqtt::QOS_1];

    let sub_opts = vec![mqtt::SubscribeOptions::with_retain_as_published(); topics.len()];

    mqtt_client.subscribe_many_with_options(&topics, &qos, &sub_opts, None).await?;


    let redis_url = env::var("REDIS_URL").unwrap_or("redis://:CkBdIsCoVeRy@redis".to_string());

    // redis context
    let redis_client = redis::Client::open(redis_url)?;
    let mut con = redis_client.get_tokio_connection().await?;

    //mqtt context
    let mqtt_tx = tokio::spawn(async move {
        while let Some(msg_opt) = mqtt_con.next().await {
            if let Some(msg) = msg_opt {
                match msg.topic() {
                    "peer/online" => {
                        if let Ok(msg) = serde_json::from_slice::<PeerInfo>(msg.payload()) {
                            info!("Received Online peer, {:?}", msg);
                            if let Err(error) = online_tx.send(msg).await {
                                error!("Failed to send peer to online_tx, error: {:?}", error);
                            }
                        }
                    },
                    "peer/online2" => {
                        if let Ok(msg) = serde_json::from_slice::<PeerInfo>(msg.payload()) {
                            info!("Received Online peer, {:?}", msg);
                            if let Err(error) = online_tx.send(msg).await {
                                error!("Failed to send peer to online_tx, error: {:?}", error);
                            }
                        }
                    },
                    "peer/reachable" => {
                        if let Ok(msg) = serde_json::from_slice::<ReachableInfo>(msg.payload()) {
                            info!("Received Reachable peer, {:?}", msg);
                            if let Err(error) = reachable_tx.send(msg).await {
                                error!("Failed to send peer to reachable_tx, error: {:?}", error);
                            }
                        } else {
                            error!("Not a valid reachable!");
                        }
                    },
                    "peer/unknown" => {
                        if let Ok(msg) = serde_json::from_slice::<NodeMetaInfo>(msg.payload()) {
                            //info!("Received Unknown peer, {:?}", msg);
                            if let Err(error) = unknown_tx.send(msg).await {
                                error!("Failed to send peer to unknown_tx, error: {:?}", error);
                            }
                        }
                    },
                    _ => { // other channel
                    info!("Got message from: {}, ignored", msg.topic());
                    }
                }
            } else {
                panic!("Lost connection! Attempting reconnect...");
            }
        }
        Ok::<(), mqtt::Error>(())
    });

    let reachable_broadcast_interval = tokio::time::sleep(Duration::from_secs(marci_broadcast_interval));
    tokio::pin!(reachable_broadcast_interval);


    loop {
        tokio::select! {
            Some(msg) = online_rx.recv() => {
                let peer: PeerInfo = msg;
                let online_key = format!(online_peer_key_format!(), peer.info.peer_id);
                let online2_key = format!(online2_peer_key_format!(), peer.info.peer_id);
                let unknown_key = format!(unknown_peer_key_format!(), peer.info.peer_id);
                let last_seen_key = format!(peer_seen_key_format!(), peer.info.peer_id);
                if peer.is_ex {
                    con.set_ex(online_key.clone(), peer.version.clone(), ckb_node_ex_timeout).await?;
                    con.set_ex(online2_key.clone(), peer.version, ckb_node_default_timeout).await?;
                } else {
                    con.set_ex(online_key.clone(), peer.version.clone(), ckb_node_default_timeout).await?;
                }
                con.set(last_seen_key, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await?;
                con.del(unknown_key).await?;
            },
            Some(msg) = unknown_rx.recv() => {
                let peer: NodeMetaInfo = msg;
                let unknown_key = format!(unknown_peer_key_format!(), peer.peer_id);
                let reachable_key = format!(reachable_peer_key_format!(), peer.peer_id);
                let last_seen_key = format!(peer_seen_key_format!(), peer.peer_id);
                let online_key = format!(online_peer_key_format!(), peer.peer_id);
                if let Ok(version) = con.get::<String, String>(online_key.clone()).await {
                    debug!("Online key exists! version: {}", version);
                    con.expire::<String, usize>(online_key, ckb_node_default_timeout);
                } else {
                    debug!("Try verify witnesses...");
                    let witnesses : usize = con.scard(format!(reachable_peer_key_format!(), peer.peer_id)).await.unwrap_or_default();
                    debug!("Witnesses of {} is {}", peer.peer_id, witnesses);
                    if witnesses >= ckb_node_default_witnesses {
                        info!("unknown peer {} upgraded into online since witnesses = {}", peer.peer_id, witnesses);
                        con.set_ex(unknown_key, "unknown", ckb_node_unknown_default_timeout).await?;
                        con.del(reachable_key).await?;
                    }
                }

                con.set(last_seen_key, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await?;
            },
            Some(msg) = reachable_rx.recv() => {
                let peer: ReachableInfo = msg;
                let reachable_key = format!(reachable_peer_key_format!(), peer.peer.peer_id);
                let last_seen_key = format!(peer_seen_key_format!(), peer.peer.peer_id);
                let network_quick_key = format!(peer_network_quick_key_format!(), peer.peer.peer_id, peer.peer.network.into_str().to_lowercase());
                con.sadd(reachable_key.clone(), peer.from.peer_id.clone()).await?;
                con.set(network_quick_key, 0).await?;
                for ip in peer.peer.addresses.iter() {
                    con.sadd(format!(peer_ip_key_format!(), peer.clone().peer.peer_id), serde_json::to_string(&ip).unwrap_or_default()).await?;
                }
                let network_key = format!(peer_network_key_format!(), peer.peer.peer_id);
                con.set(network_key, peer.peer.network.into_str()).await?;
                con.set(last_seen_key, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await?;

                // Update Ip info
                let country: String =  con.get(format!(peer_country_key_format!(), peer.peer.peer_id)).await.unwrap_or_default();
                if country.is_empty() {
                    if let Some(ip) = query_by_reachable(&peer).await {
                    con.set(format!(peer_country_key_format!(), peer.peer.peer_id), ip.country).await?;
                    con.set(format!(peer_city_key_format!(), peer.peer.peer_id), ip.city).await?;
                    con.set(format!(peer_region_key_format!(), peer.peer.peer_id), ip.region).await?;
                    con.set(format!(peer_pos_key_format!(), peer.peer.peer_id), ip.loc).await?;
                }
                }

                // Update Service Info
                con.set::<String, u64, u64>("service.last_update".to_string(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await.unwrap_or_default();
            },

            // Different timers
            () = &mut reachable_broadcast_interval => {
                let reachable_peer_keys_vec: Vec<String> = con.keys("peer.reachable.*").await?;
                if reachable_peer_keys_vec.is_empty() {
                    debug!("No reachable peer needs to call");
                }
                let peers_reachable = reachable_peer_keys_vec.iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>();

                let online_peer_keys_vec: Vec<String> = con.keys("peer.online.*").await?;
                let peers_online = online_peer_keys_vec.iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>();

                info!("Broadcasting reachable peers to dialers...");
                for peer_id in peers_reachable.iter().filter(|x| !peers_online.contains(x)) {
                    // get addresses
                    let raw_info: Vec<String> = con.smembers(format!(peer_ip_key_format!(), peer_id)).await.unwrap_or_default();
                    let mut addresses = Vec::new();
                    let network_string: String = con.get(format!(peer_network_key_format!(), peer_id)).await.unwrap_or_default();
                    for info in raw_info.iter() {
                        if let Ok(endpoint) = serde_json::from_str::<EndpointInfo>(info.as_str()) {
                            addresses.push(endpoint)
                        } else {
                            error!("{} is not a valid endpoint!", info);
                        }
                    }
                    let meta_info = NodeMetaInfo {
                        peer_id: peer_id.to_string(),
                        addresses,
                        network: CKBNetworkType::from(network_string),
                    };
                    pub_context.publish(mqtt::Message::new("peer/needs_dial", serde_json::to_string(&meta_info).unwrap_or_default(), mqtt::QOS_1)).await?;
                }
                info!("Requested {} reachable peers", peers_reachable.len());
                reachable_broadcast_interval.as_mut().reset(Instant::now() + Duration::from_secs(marci_broadcast_interval));

                // Update Service Info
                con.set::<String, u64, u64>("service.last_update".to_string(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await.unwrap_or_default();
            },
        }
    }

    warn!("exiting...");
}

extern crate core;

use rumqttc::{MqttOptions, AsyncClient, QoS, SubscribeFilter};
use std::time::UNIX_EPOCH;
use redis::{AsyncCommands, Commands, ConnectionLike};
use tokio::time::{Duration, Instant};
use ckb_discovery_types::{CKBNetworkType, EndpointInfo, NodeMetaInfo, PeerInfo, ReachableInfo};
use log::{debug, error, info, log, warn};
use std::{env};
use std::time::SystemTime;

async fn check_peer_online(con: &mut redis::aio::Connection, peer_id: String) -> bool {
    con.keys::<&String, Vec<String>>(&peer_id).await.unwrap_or_default().contains(&peer_id)
}

macro_rules! online_peer_key_format {
    () => ("peer.online.{}")
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

macro_rules! peer_witnesses_key_format {
    () => ("peer_info.{}.witnesses")
}

macro_rules! peer_network_key_format {
    () => ("peer_info.{}.network")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env::set_var("RUST_LOG", "info");
    let ckb_node_default_timeout = env::var("CKB_NODE_DEFAULT_TIMEOUT").unwrap_or("259200".to_string()).parse::<usize>()?;
    //let ckb_node_default_timeout = env::var("CKB_NODE_DEFAULT_TIMEOUT").unwrap_or("100".to_string()).parse::<usize>()?;
    let ckb_node_default_witnesses = env::var("CKB_NODE_DEFAULT_WITNESSES").unwrap_or("3".to_string()).parse::<usize>()?;

    env_logger::init();
    let (online_tx, mut online_rx) = tokio::sync::mpsc::channel::<PeerInfo>(50);
    let (reachable_tx, mut reachable_rx) = tokio::sync::mpsc::channel::<ReachableInfo>(50);

    let (unknown_tx, mut unknown_rx) = tokio::sync::mpsc::channel::<NodeMetaInfo>(50);

    let mqtt_url = env::var("MQTT_URL").unwrap_or("mqtt://localhost:1883".to_string());

    let mut mqttoptions = MqttOptions::parse_url(format!("{}?client_id=MARCI", mqtt_url))?;
    mqttoptions.set_keep_alive(Duration::from_secs(10));

    let (mut client, mut context) = AsyncClient::new(mqttoptions, 50);

    client.subscribe_many(vec!["peer/online", "peer/reachable", "peer/unknown"].into_iter().map(|x| {
        SubscribeFilter::new(x.to_string(), QoS::AtMostOnce)
    }).collect::<Vec<_>>()).await?;



    // redis context
    let redis_client = redis::Client::open("redis://:CkBdIsCoVeRy@127.0.0.1")?;
    let mut con = redis_client.get_tokio_connection().await?;

    //mqtt context
    let mqtt_tx = tokio::spawn(async move {
        while let Ok(notification) = context.poll().await {
            match notification {
                rumqttc::Event::Incoming(rumqttc::Packet::Publish(raw_message)) => { // received a new message
                    match raw_message.topic.as_str() {
                        "peer/online" => {
                            if let Ok(msg) = serde_json::from_slice::<PeerInfo>(raw_message.payload.as_ref()) {
                                info!("Received Online peer, {:?}", msg);
                                if let Err(error) = online_tx.send(msg).await {
                                    error!("Failed to send peer to online_tx, error: {:?}", error);
                                }
                            }
                        },
                        "peer/reachable" => {
                            if let Ok(msg) = serde_json::from_slice::<ReachableInfo>(raw_message.payload.as_ref()) {
                                info!("Received Reachable peer, {:?}", msg);
                                if let Err(error) = reachable_tx.send(msg).await {
                                    error!("Failed to send peer to reachable_tx, error: {:?}", error);
                                }
                            }
                        },
                        "peer/unknown" => {
                            if let Ok(msg) = serde_json::from_slice::<NodeMetaInfo>(raw_message.payload.as_ref()) {
                                info!("Received Unknown peer, {:?}", msg);
                                if let Err(error) = unknown_tx.send(msg).await {
                                    error!("Failed to send peer to unknown_tx, error: {:?}", error);
                                }
                            }
                        },
                        _ => { // other channel
                            info!("Got message from: {:?}, ignored", raw_message.topic);
                        }
                    }
                }
                _ => {
                    debug!("GENERIC MQTT NOTIFY: {:?}", notification)
                }
            }
        }
        error!("MQTT Context exited...");
    });

    let unknown_broadcast_interval = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(unknown_broadcast_interval);

    let online_broadcast_interval = tokio::time::sleep(Duration::from_secs(15)); // refresh online time every 12Hrs
    tokio::pin!(online_broadcast_interval);

    let reachable_broadcast_interval = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(reachable_broadcast_interval);

    loop {
        tokio::select! {
            Some(msg) = online_rx.recv() => {
                let peer: PeerInfo = msg;
                let online_key = format!(online_peer_key_format!(), peer.info.peer_id);
                let last_seen_key = format!(peer_seen_key_format!(), peer.info.peer_id);
                con.set_ex(online_key.clone(), peer.version, ckb_node_default_timeout).await?;
                con.set(last_seen_key, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await?;
                //con.del(format!(reachable_peer_key_format!(), peer.info.peer_id)).await?;
            },
            Some(msg) = unknown_rx.recv() => {
                let peer: NodeMetaInfo = msg;
                let unknown_key = format!(unknown_peer_key_format!(), peer.peer_id);
                let last_seen_key = format!(peer_seen_key_format!(), peer.peer_id);
                let online_key = format!(online_peer_key_format!(), peer.peer_id);
                if let Ok(version) = con.get::<String, String>(online_key.clone()).await {
                    info!("Online key exists! version: {}", version);
                    con.expire::<String, usize>(online_key, ckb_node_default_timeout);
                } else {
                    info!("Try verify witnesses...");
                    let witnesses : usize = con.scard(format!(reachable_peer_key_format!(), peer.peer_id)).await.unwrap_or_default();
                    info!("Witnesses of {} is {}", peer.peer_id, witnesses);
                    if witnesses >= ckb_node_default_witnesses {
                        info!("unknown peer {} upgraded into online since witnesses = {}", peer.peer_id, witnesses);
                        con.set_ex(unknown_key, "unknown", ckb_node_default_timeout).await?;
                        con.del(format!(reachable_peer_key_format!(), peer.peer_id)).await?;
                        //con.del(format!(reachable_peer_key_format!(), peer.peer_id)).await?;
                    }
                }

                con.set(last_seen_key, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await?;
            },
            Some(msg) = reachable_rx.recv() => {
                let peer: ReachableInfo = msg;
                let reachable_key = format!(reachable_peer_key_format!(), peer.peer.peer_id);
                let last_seen_key = format!(peer_seen_key_format!(), peer.peer.peer_id);
                con.sadd(reachable_key.clone(), peer.from.peer_id).await?;
                for ip in peer.peer.addresses {
                    con.sadd(format!(peer_ip_key_format!(), peer.peer.peer_id), serde_json::to_string(&ip).unwrap_or_default()).await?;
                }
                let network_key = format!(peer_network_key_format!(), peer.peer.peer_id);
                con.set(network_key, peer.peer.network.into_str()).await?;
                con.set(last_seen_key, SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()).await?;
            },

            // Different timers
            () = &mut unknown_broadcast_interval => {
                let unknown_peer_keys_vec: Vec<String> = con.keys("peer.unknown.*").await?;
                if unknown_peer_keys_vec.is_empty() {
                    info!("No unknown peer needs to call");
                }
                let peers_unknown = unknown_peer_keys_vec.iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>();

                let online_peer_keys_vec: Vec<String> = con.keys("peer.online.*").await?;
                if online_peer_keys_vec.is_empty() {
                    info!("No reachable peer needs to call");
                }
                let peers_online = online_peer_keys_vec.iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>();

                info!("Broadcasting unknown peers to dialers...");
                for peer_id in peers_unknown.into_iter().filter(|x| !peers_online.contains(x)) {
                    info!("Request for dial {}...", peer_id);
                    // get addresses
                    let raw_info: Vec<String> = con.smembers(format!(peer_ip_key_format!(), peer_id)).await?;
                    let mut addresses = Vec::new();
                    let network_string: String = con.get(format!(peer_network_key_format!(), peer_id)).await?;
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
                    if let Err(err) = client.publish("peer/needs_dial", QoS::AtMostOnce, true, serde_json::to_string(&meta_info).unwrap_or_default()).await {
                        error!("Failed to publish reachable peer {}, detail: {:?}", peer_id, err);
                    }
                }
                unknown_broadcast_interval.as_mut().reset(Instant::now() + Duration::from_secs(15));
            },

            () = &mut online_broadcast_interval => {
                let online_peer_keys_vec: Vec<String> = con.keys("peer.online.*").await?;
                if online_peer_keys_vec.is_empty() {
                    info!("No reachable peer needs to call");
                }
                let peers_online = online_peer_keys_vec.iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>();

                info!("Broadcasting unknown peers to dialers...");
                for peer_id in peers_online {
                    info!("Request for dial {}...", peer_id);
                    // get addresses
                    let raw_info: Vec<String> = con.smembers(format!(peer_ip_key_format!(), peer_id)).await?;
                    let mut addresses = Vec::new();
                    let network_string: String = con.get(format!(peer_network_key_format!(), peer_id)).await?;
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
                    if let Err(err) = client.publish("peer/needs_dial", QoS::AtMostOnce, true, serde_json::to_string(&meta_info).unwrap_or_default()).await {
                        error!("Failed to publish reachable peer {}, detail: {:?}", peer_id, err);
                    }
                }
                online_broadcast_interval.as_mut().reset(Instant::now() + Duration::from_secs(15));
            },

            () = &mut reachable_broadcast_interval => {
                let reachable_peer_keys_vec: Vec<String> = con.keys("peer.reachable.*").await?;
                if reachable_peer_keys_vec.is_empty() {
                    info!("No reachable peer needs to call");
                }
                let peers_reachable = reachable_peer_keys_vec.iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>();

                let online_peer_keys_vec: Vec<String> = con.keys("peer.online.*").await?;
                if online_peer_keys_vec.is_empty() {
                    info!("No reachable peer needs to call");
                }
                let peers_online = online_peer_keys_vec.iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>();

                info!("Broadcasting reachable peers to dialers...");
                for peer_id in peers_reachable.into_iter().filter(|x| !peers_online.contains(x)) {
                    info!("Request for dial {}...", peer_id);
                    // get addresses
                    let raw_info: Vec<String> = con.smembers(format!(peer_ip_key_format!(), peer_id)).await?;
                    let mut addresses = Vec::new();
                    let network_string: String = con.get(format!(peer_network_key_format!(), peer_id)).await?;
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
                    if let Err(err) = client.publish("peer/needs_dial", QoS::AtMostOnce, true, serde_json::to_string(&meta_info).unwrap_or_default()).await {
                        error!("Failed to publish reachable peer {}, detail: {:?}", peer_id, err);
                    }
                }
                reachable_broadcast_interval.as_mut().reset(Instant::now() + Duration::from_secs(30));
            },
        }
    }

    warn!("exiting...");
}

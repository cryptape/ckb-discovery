use async_trait::async_trait;
use ckb_types::bytes::Bytes;
use ckb_types::packed;
use ckb_types::prelude::*;
use p2p::traits::{ServiceHandle, ServiceProtocol, SessionProtocol};
use p2p::context::{ProtocolContext, ProtocolContextMutRef, ServiceContext};
use p2p::service::{ProtocolHandle, ProtocolMeta, ServiceError, ServiceEvent, TargetProtocol, TargetSession};
use log::{info, error, debug};
use p2p::builder::MetaBuilder;
use std::time::{Duration, Instant};
use p2p::bytes::BytesMut;
use p2p::multiaddr::Multiaddr;
use tokio_util::codec::{Encoder, LengthDelimitedCodec};
use ckb_discovery_types::{CKBNetworkType, PeerInfo, ReachableInfo};
use paho_mqtt as mqtt;
use paho_mqtt::QOS_1;
use crate::compress::{compress, decompress};
use crate::message::build_discovery_get_nodes;
use crate::network::{addr_to_endpoint, addr_to_node_meta, get_bootnodes};
use crate::support_protocols::SupportProtocols;

const DISCOVERY_TOKEN: u64 = 1;
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(10);

pub struct Handler {
    network_type: CKBNetworkType,
    mqtt_context: mqtt::AsyncClient,
}

impl Clone for Handler {
    fn clone(&self) -> Self {
        Self {
            network_type: self.network_type.clone(),
            mqtt_context: self.mqtt_context.clone().to_owned(),
        }
    }
}


impl Handler {
    pub fn new(network_type: CKBNetworkType, mqtt_context: mqtt::AsyncClient) -> Self {
        Self {
            network_type,
            mqtt_context
        }
    }

    /// Convert Handler into P2PProtocolMeta
    pub fn build_protocol_metas(&self) -> Vec<ProtocolMeta> {
        vec![
            {
                let meta_builder: MetaBuilder = SupportProtocols::Identify.into();
                meta_builder
                    .service_handle(move || ProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
            {
                let meta_builder: MetaBuilder = SupportProtocols::Discovery.into();
                meta_builder
                    .service_handle(move || ProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
            {
                // Necessary to communicate with CKB full node
                let meta_builder: MetaBuilder = SupportProtocols::Sync.into();
                meta_builder
                    // Only Timer, Sync, Relay make compress
                    .before_send(compress)
                    .before_receive(|| Some(Box::new(decompress)))
                    .service_handle(move || ProtocolHandle::Callback(Box::new(self.clone())))
                    .build()
            },
        ]
    }

    async fn connected_discovery(&mut self, context: &ProtocolContextMutRef<'_>, protocol_version: &str) {
        let discovery_get_node_message = build_discovery_get_nodes(None, 1000u32, 1u32);
        if protocol_version == "0.0.1" {
            let mut codec = LengthDelimitedCodec::new();
            let mut bytes = BytesMut::new();
            codec
                .encode(discovery_get_node_message.as_bytes(), &mut bytes)
                .expect("encode must be success");
            let message_bytes = bytes.freeze();
            if let Err(error) = context.send_message(message_bytes).await {
                error!("Failed to send discovery to {}", context.session.address);
            }
        } else {
            let message_bytes = discovery_get_node_message.as_bytes();
            if let Err(error) = context.send_message(message_bytes).await {
                error!("Failed to send discovery to {}", context.session.address);
            }
        }
    }

    fn received_identify(&mut self, context: ProtocolContextMutRef, data: Bytes) {
        info!("Received Identify from {:?}!", context.session.address);
        if let Ok(message) = packed::IdentifyMessage::from_compatible_slice(data.as_ref()) {
            match packed::Identify::from_compatible_slice(message.identify().raw_data().as_ref()) {
                Ok(identify_payload) => {
                    let client_version_vec: Vec<u8> =
                        identify_payload.client_version().unpack();
                    let client_version =
                        String::from_utf8_lossy(&client_version_vec).to_string();
                    let client_flag: u64 =  identify_payload.flag().unpack();
                    // protocol is private mod in ckb, use the bitflag map directly
                    // since a light node can't provide LIGHT_CLIENT serv but full node can, use this as a workaround
                    let full = (client_flag & 0b10000) == 0b10000;
                    info!("Received IdentifyMessage, address: {}, time: {:?}, version: {}, is_full:{}", context.session.address, Instant::now(), client_version, full);
                    let peer_info = PeerInfo {
                        info: addr_to_node_meta(&context.session.address, self.network_type),
                        version: client_version,
                        full,
                    };
                    let mqtt_context = self.mqtt_context.clone().to_owned();

                    tokio::spawn(async move {
                        if let Err(error) = mqtt_context.publish(mqtt::Message::new("peer/online", serde_json::to_vec(&peer_info).unwrap_or_default(), mqtt::QOS_1)).await {
                            error!("Failed to publish peer {:?} to online!", peer_info);
                        }
                    });
                },
                Err(err) => {
                    log::error!("Received invalid Identify Payload, address: {}, error: {:?}", context.session.address, err);
                }
            }
        } else {
            error!("Failed to decode message!");
        }
    }

    fn received_discovery(&mut self, context: ProtocolContextMutRef, data: Bytes) {
        if let Ok(message) = packed::DiscoveryMessage::from_compatible_slice(data.as_ref()) {
            match message.payload().to_enum() {
                packed::DiscoveryPayloadUnion::Nodes(discovery_nodes) => {
                    info!("Received DiscoveryMessages Nodes, address: {}, nodes.len: {}", context.session.address, discovery_nodes.items().len());
                    let from = addr_to_node_meta(&context.session.address, self.network_type);
                    for node in discovery_nodes.items() {
                        let from_shadow = from.clone();
                        if node.addresses().is_empty() {
                            continue
                        }
                        let addresses = node.addresses().unpack();
                        let mut meta = addr_to_node_meta(&Multiaddr::try_from(addresses.get(0).unwrap().to_vec()).unwrap(), self.network_type);
                        if addresses.len() > 1 {
                            for address in &addresses[1..] {
                                let endpoint =  Multiaddr::try_from(address.to_vec()).unwrap();
                                meta.addresses.push(addr_to_endpoint(&endpoint));
                            }
                        }
                        let mqtt_context = self.mqtt_context.clone().to_owned();

                        tokio::spawn(async move {
                            if let Err(error) = mqtt_context.publish(
                                mqtt::Message::new(
                                    "peer/reachable",
                                    serde_json::to_vec(
                                        &ReachableInfo::new(
                                            meta.clone(),
                                            from_shadow.clone())
                                    ).unwrap_or_default(),
                                    QOS_1)
                            ).await {
                                error!("Failed to publish peer {:?} to online! error: {:?}", meta, error);
                            }
                        });
                    }



                },
                packed::DiscoveryPayloadUnion::GetNodes(_discovery_get_nodes) => {} // discard
            }
        }
    }
}

#[async_trait]
impl ServiceHandle for Handler {
    /// Handling runtime errors
    async fn handle_error(&mut self, _control: &mut ServiceContext, error: ServiceError) {
        match &error {
            ServiceError::DialerError { address, error } => {
                // failed to dial, report unknown
                debug!("failed to dail {:?}", address.to_string());
                if let Err(error) = self.mqtt_context.publish(mqtt::Message::new("peer/unknown", serde_json::to_vec(&addr_to_node_meta(address, self.network_type)).unwrap_or_default(), mqtt::QOS_1)).await {
                    error!("Failed to publish address to mqtt!");
                }
            },
            ServiceError::ProtocolSelectError {..} => {} //discard this error
            _ => {
                error!("ServiceHandler detect error: {:?}", error);
            }
        }
    }

    /// Handling session establishment and disconnection events
    async fn handle_event(&mut self, _control: &mut ServiceContext, event: ServiceEvent) {
        match event {
            ServiceEvent::SessionOpen { session_context: session} => {
                debug!("Session open: {:?}", session);
            },
            ServiceEvent::SessionClose { session_context: session} => {
                debug!("Session close: {:?}", session.clone());
            },
            _ => {
                debug!("Session event: {:?}", event);
            }, // we don't care about this
        }
    }
}



#[async_trait]
impl ServiceProtocol for Handler {
    async fn init(&mut self, context: &mut ProtocolContext) {
        let bootnodes = get_bootnodes(self.network_type);
        for node in bootnodes {
            debug!("Trying to dial {}", &node);
            let dial_res = context.dial(node.clone(), p2p::service::TargetProtocol::All).await;
            debug!("Dial {} result: {:?}", &node, &dial_res);
        }
    }

    async fn connected(&mut self, context: ProtocolContextMutRef<'_>, protocol_version: &str) {
        debug!(
            "Handler open protocol, protocol_name: {} address: {}",
            context
                .protocols()
                .get(&context.proto_id())
                .map(|p| p.name.as_str())
                .unwrap_or_default(),
            context.session.address
        );

        if context.proto_id() == SupportProtocols::Discovery.protocol_id() {
            context.set_service_notify(SupportProtocols::Discovery.protocol_id(), DISCOVERY_INTERVAL, DISCOVERY_TOKEN).await.unwrap_or_default();
            self.connected_discovery(&context, protocol_version).await;
        }
    }

    async fn disconnected(&mut self, context: ProtocolContextMutRef<'_>) {
        debug!(
            "Handler close protocol, protocol_name: {}, address: {:?}",
            context
                .protocols()
                .get(&context.proto_id())
                .map(|p| p.name.as_str())
                .unwrap_or_default(),
            context.session.address
        );
        // Other ops...
    }

    async fn received(&mut self, context: ProtocolContextMutRef<'_>, data: Bytes) {
        if context.proto_id == SupportProtocols::Discovery.protocol_id() {
            self.received_discovery(context, data);
        } else if context.proto_id == SupportProtocols::Identify.protocol_id() {
            self.received_identify(context, data);
        }
    }

    async fn notify(&mut self, context: &mut ProtocolContext, token: u64) {
        match token {
            DISCOVERY_TOKEN => {
                let discovery_get_node_message = build_discovery_get_nodes(None, 1000u32, 1u32);
                let message_bytes = discovery_get_node_message.as_bytes();
                if let Err(error) = context.quick_filter_broadcast(TargetSession::All, SupportProtocols::Discovery.protocol_id(), message_bytes).await {
                    error!("Failed to broadcast discovery , error: {}", error);
                }
            },
            _ => {

            }
        }
    }


}
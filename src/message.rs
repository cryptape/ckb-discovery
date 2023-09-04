use ckb_types::packed;
use ckb_types::prelude::*;
use p2p::multiaddr::Multiaddr;

pub fn build_discovery_get_nodes(
    listening_port: Option<u16>,
    max_nodes: u32,
    self_defined_flag: u32,
) -> packed::DiscoveryMessage {
    let discovery_payload = packed::DiscoveryPayload::new_builder()
        .set(
            packed::GetNodes::new_builder()
                .listen_port({
                    match listening_port {
                        None => packed::PortOpt::default(),
                        Some(port) => packed::PortOpt::new_builder()
                            .set(Some({
                                packed::Uint16::from_slice(&port.to_le_bytes()).unwrap()
                            }))
                            .build(),
                    }
                })
                .count(max_nodes.pack())
                .version(self_defined_flag.pack())
                .build(),
        )
        .build();
    packed::DiscoveryMessage::new_builder()
        .payload(discovery_payload)
        .build()
}

pub fn build_discovery_nodes(
    active_push: bool,
    addresses: Vec<Multiaddr>,
) -> packed::DiscoveryMessage {
    let nodes = addresses
        .into_iter()
        .map(|address| {
            let bytes = packed::Bytes::new_builder()
                .set(address.to_vec().into_iter().map(Into::into).collect())
                .build();
            packed::Node::new_builder()
                .addresses(vec![bytes].pack())
                .build()
        })
        .collect::<Vec<_>>();
    let discovery_payload = packed::DiscoveryPayload::new_builder()
        .set(
            packed::Nodes::new_builder()
                .announce(active_push.pack())
                .items(packed::NodeVec::new_builder().set(nodes).build())
                .build(),
        )
        .build();
    packed::DiscoveryMessage::new_builder()
        .payload(discovery_payload)
        .build()
}
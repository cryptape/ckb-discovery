use p2p::builder::MetaBuilder;
use p2p::service::{ProtocolHandle, ProtocolMeta};
use p2p::traits::ServiceProtocol;
use p2p::ProtocolId;
use tokio_util::codec::length_delimited;

#[derive(Clone, Debug)]
pub enum SupportProtocols {
    /// Ping: as a health check for ping/pong
    Ping,
    /// Discovery: used to communicate with any node with any known node address,
    /// to build a robust network topology as much as possible.
    Discovery,
    /// Identify: the first protocol opened when the nodes are interconnected,
    /// used to obtain the features, versions, and observation addresses supported by the other node.
    ///
    /// [RFC](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0012-node-discovery/0012-node-discovery.md)
    Identify,
    /// Feeler: used to detect whether the address is valid.
    ///
    /// [RFC](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0007-scoring-system-and-network-security/0007-scoring-system-and-network-security.md#feeler-connection)
    /// [Eclipse Attacks on Bitcoin's Peer-to-Peer Network](https://cryptographylab.bitbucket.io/slides/Eclipse%20Attacks%20on%20Bitcoin%27s%20Peer-to-Peer%20Network.pdf)
    Feeler,
    /// Disconnect message: used to give the remote node a debug message when the node decides to disconnect.
    /// This message must be as quick as possible, otherwise the message may not be sent. So, use a separate protocol to support it.
    DisconnectMessage,
    /// Sync: ckb's main communication protocol for synchronize all blocks.
    ///
    /// [RFC](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0004-ckb-block-sync/0004-ckb-block-sync.md)
    Sync,
    /// Relay: ckb's main communication protocol for synchronizing latest transactions and blocks.
    ///
    /// [RFC](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0004-ckb-block-sync/0004-ckb-block-sync.md#new-block-announcement)
    Relay,
    /// New Relay
    RelayV2,
    /// Time: A protocol used for node pairing that warns if there is a large gap between the local time and the remote node.
    Time,
    /// Alert: A protocol reserved by the Nervos Foundation to publish network-wide announcements.
    /// Any information sent from the protocol is verified by multi-signature
    Alert,
}

impl SupportProtocols {
    /// Protocol id
    pub fn protocol_id(&self) -> ProtocolId {
        match self {
            SupportProtocols::Ping => 0,
            SupportProtocols::Discovery => 1,
            SupportProtocols::Identify => 2,
            SupportProtocols::Feeler => 3,
            SupportProtocols::DisconnectMessage => 4,
            SupportProtocols::Sync => 100,
            SupportProtocols::Relay => 101,
            SupportProtocols::Time => 102,
            SupportProtocols::RelayV2 => 103,
            SupportProtocols::Alert => 110,
        }
        .into()
    }

    /// Protocol name
    pub fn name(&self) -> String {
        match self {
            SupportProtocols::Ping => "/ckb/ping",
            SupportProtocols::Discovery => "/ckb/discovery",
            SupportProtocols::Identify => "/ckb/identify",
            SupportProtocols::Feeler => "/ckb/flr",
            SupportProtocols::DisconnectMessage => "/ckb/disconnectmsg",
            SupportProtocols::Sync => "/ckb/syn",
            SupportProtocols::Relay => "/ckb/rel",
            SupportProtocols::RelayV2 => "/ckb/relay",
            SupportProtocols::Time => "/ckb/tim",
            SupportProtocols::Alert => "/ckb/alt",
        }
        .to_owned()
    }

    /// Support versions
    pub fn support_versions(&self) -> Vec<String> {
        match self {
            SupportProtocols::Ping => vec!["2".to_owned(), "3".to_owned()],
            SupportProtocols::Discovery => vec!["2".to_owned(), "2.1".to_owned(), "3".to_owned()],
            SupportProtocols::Identify => vec!["2".to_owned(), "3".to_owned()],
            SupportProtocols::Feeler => vec!["2".to_owned(), "3".to_owned()],
            SupportProtocols::DisconnectMessage => {
                vec!["2".to_owned(), "3".to_owned()]
            }
            SupportProtocols::Sync => vec!["2".to_owned(), "3".to_owned()],
            SupportProtocols::Relay => vec!["3".to_owned()],
            SupportProtocols::Time => vec!["2".to_owned(), "3".to_owned()],
            SupportProtocols::Alert => vec!["2".to_owned(), "3".to_owned()],
            SupportProtocols::RelayV2 => vec!["2".to_owned(), "3".to_owned()],
        }
    }

    /// Protocol message max length
    pub fn max_frame_length(&self) -> usize {
        match self {
            SupportProtocols::Ping => 1024,              // 1   KB
            SupportProtocols::Discovery => 512 * 1024,   // 512 KB
            SupportProtocols::Identify => 2 * 1024,      // 2   KB
            SupportProtocols::Feeler => 1024,            // 1   KB
            SupportProtocols::DisconnectMessage => 1024, // 1   KB
            SupportProtocols::Sync => 2 * 1024 * 1024,   // 2   MB
            SupportProtocols::Relay | SupportProtocols::RelayV2 => 4 * 1024 * 1024, // 4   MB
            SupportProtocols::Time => 1024,              // 1   KB
            SupportProtocols::Alert => 128 * 1024,       // 128 KB
        }
    }

    /// Builder with service handle
    // a helper fn to build `ProtocolMeta`
    pub fn build_meta_with_service_handle<
        SH: FnOnce() -> ProtocolHandle<Box<dyn ServiceProtocol + Send + 'static + Unpin>>,
    >(
        self,
        service_handle: SH,
    ) -> ProtocolMeta {
        let meta_builder: MetaBuilder = self.into();
        meta_builder.service_handle(service_handle).build()
    }
}

impl From<SupportProtocols> for MetaBuilder {
    fn from(p: SupportProtocols) -> Self {
        let max_frame_length = p.max_frame_length();
        MetaBuilder::default()
            .id(p.protocol_id())
            .support_versions(p.support_versions())
            .name(move |_| p.name())
            .codec(move || {
                Box::new(
                    length_delimited::Builder::new()
                        .max_frame_length(max_frame_length)
                        .new_codec(),
                )
            })
    }
}

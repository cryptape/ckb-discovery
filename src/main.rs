use std::env;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::error::Error;
use std::time::Duration;
use ckb_discovery_types::{CKBNetworkType, NodeMetaInfo, PeerInfo};
use log::{debug, error, info};
use p2p::multiaddr::{MultiAddr, Multiaddr};
use p2p::secio::SecioKeyPair;
use p2p::service::{TargetProtocol, TargetSession};
use p2p::SessionId;
use p2p::yamux::Config;
use ckb_discovery::handler::Handler;
use ckb_discovery::network::{meta_info_to_addr};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mqtt_url = env::var("MQTT_URL").unwrap_or("mqtt://localhost:1883".to_string());

    let mut mqttoptions = MqttOptions::parse_url(format!("{}?client_id=CKB_DISCOVERY", mqtt_url))?;
    mqttoptions.set_keep_alive(Duration::from_secs(30)).set_clean_session(true);

    let (mut client, mut context) = AsyncClient::new(mqttoptions, 30);
    client.subscribe("peer/needs_dial", QoS::AtMostOnce).await.expect("MQTT Failed to subscribe");

    let builder_fn = |network: CKBNetworkType|{
        let mqtt_ctx = client.clone().to_owned();
        let mut service_builder = p2p::builder::ServiceBuilder::new();
        let handler = Handler::new(network.clone(), mqtt_ctx);

        for meta in handler.build_protocol_metas() {
            service_builder = service_builder.insert_protocol(meta);
        }
        service_builder
            .forever(true)
            .timeout(Duration::from_secs(15))
            .key_pair(SecioKeyPair::secp256k1_generated())
            .yamux_config(Config::default())
            .set_recv_buffer_size( 24 * 1024 * 1024 )
            .set_send_buffer_size(24 * 1024 * 1024)
            .timeout(Duration::from_secs(30))
            .build(handler)
    };

    let mut mirana_service = builder_fn(CKBNetworkType::Mirana);
    let mut pudge_service = builder_fn(CKBNetworkType::Pudge);


    let mirana_controller = mirana_service.control().to_owned();
    let pudge_controller = pudge_service.control().to_owned();

    let service_tx = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = mirana_service.run() => {

                },

                _ = pudge_service.run() => {

                },
            }
        }
    });

    let mqtt_tx = tokio::spawn(async move {
        while let Ok(notification) = context.poll().await {
            match notification {
                rumqttc::Event::Incoming(rumqttc::Packet::Publish(raw_message)) => { // received a new message
                    match raw_message.topic.as_str() {
                        "peer/needs_dial" => { // received a dial signal
                            match serde_json::from_slice::<NodeMetaInfo>(raw_message.payload.as_ref()) {
                                Ok(node) => {
                                    info!("Start dial {:?}", node);
                                    let addrs = meta_info_to_addr(&node);
                                    for addr in addrs {
                                        if let Ok(addr) = addr {
                                            let res = match node.network {
                                                CKBNetworkType::Pudge => {
                                                    pudge_controller.dial(addr.clone(), TargetProtocol::All).await
                                                },
                                                CKBNetworkType::Mirana => {
                                                    mirana_controller.dial(addr.clone(), TargetProtocol::All).await
                                                },
                                                _ => unreachable!(),
                                            } ;
                                            if res.is_err() {
                                                error!("Failed to dial {:?}!", addr.to_string());
                                            }
                                        }
                                    }
                                },
                                Err(error) => {
                                    error!("{:?} not a valid multiaddr", raw_message.payload);
                                }
                            }
                        },
                        _ => {  // other channel
                        }
                    }
                }
                _ => {
                    debug!("GENERIC MQTT NOTIFY: {:?}", notification)
                }
            }
        }
        panic!("MQTT Context exited, Maybe service not ready...");
    });

    mqtt_tx.await?;
    service_tx.await?;

    Ok(())
}

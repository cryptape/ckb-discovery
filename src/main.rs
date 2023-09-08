use std::env;
use paho_mqtt as mqtt;
use std::error::Error;
use std::time::Duration;
use futures::StreamExt;
use ckb_discovery_types::{CKBNetworkType, NodeMetaInfo, PeerInfo};
use log::{debug, error, info};
use nanoid::nanoid;
use p2p::multiaddr::{MultiAddr, Multiaddr};
use p2p::secio::SecioKeyPair;
use p2p::service::{TargetProtocol, TargetSession};
use p2p::SessionId;
use p2p::yamux::Config;
use paho_mqtt::{QOS_1, QOS_2};
use ckb_discovery::handler::Handler;
use ckb_discovery::network::{meta_info_to_addr};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mqtt_url = env::var("MQTT_URL").unwrap_or("mqtt:1883".to_string());
    let mqtt_user = env::var("MQTT_USER").unwrap_or("ckb".to_string());
    let mqtt_pass = env::var("MQTT_PASS").unwrap_or("ckbdiscovery".to_string());

    let create_ops = mqtt::CreateOptionsBuilder::new()
        .server_uri(mqtt_url.clone())
        .max_buffered_messages(500)
        .client_id(format!("ckb-discovery-{}", nanoid!()))
        .finalize();

    let mut mqtt_client = mqtt::AsyncClient::new(create_ops).expect("Failed to create MQTT client");

    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .clean_start(true)
        .keep_alive_interval(Duration::from_millis(300))
        .automatic_reconnect(Duration::from_millis(500), Duration::from_millis(2000))
        .user_name(mqtt_user)
        .password(mqtt_pass)
        .properties(mqtt::properties![mqtt::PropertyCode::SessionExpiryInterval => 5000])
        .finalize();

    mqtt_client.connect(conn_opts).await?;

    println!("Connected to MQTT!");

    let mut mqtt_con = mqtt_client.get_stream(None);

    let topic = "peer/needs_dial";

    mqtt_client.subscribe(topic, QOS_1).await?;



    let builder_fn = |network: CKBNetworkType|{
        let mqtt_ctx = mqtt_client.clone().to_owned();
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
        while let Some(msg_opt) = mqtt_con.next().await {
            if let Some(msg) = msg_opt {
                match msg.topic() {
                    "peer/needs_dial" => { // received a dial signal
                        match serde_json::from_slice::<NodeMetaInfo>(msg.payload()) {
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
                                error!("{:?} not a valid multiaddr", msg.payload());
                            }
                        }
                    },
                    _ => {  // other channel
                    }
                }
            } else {
                panic!("Lost connection! Attempting reconnect...");
            }
        }
        panic!("MQTT Context exited, Maybe service not ready...");
    });

    mqtt_tx.await?;
    service_tx.await?;

    Ok(())
}

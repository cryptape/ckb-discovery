mod model;

use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use actix_cors::Cors;
use actix_web::web::Data;
use actix_web::{http, web, App, HttpResponse, HttpServer, Responder};
use std::time::Duration;
use actix_web::http::header::{CacheControl, CacheDirective};
use chrono::{Utc, DateTime};
use redis::{AsyncCommands, Commands};
use regex::Regex;
use tokio::sync::Mutex;
use crate::model::{Peer, QueryParams, ServiceStatus};
use clap::Arg;
use ckb_discovery_types::CKBNetworkType;

struct ServiceData {
    client: Data<Mutex<redis::aio::Connection>>,
    last_cache_update: Mutex<SystemTime>,
    data: Mutex<Vec<Peer>>,
}

// Define a handler function for the "/peer" endpoint
async fn peer_handler(
    query_params: web::Query<QueryParams>,
    mut data: Data<ServiceData>,
) -> impl Responder {
    if data.data.lock().await.is_empty() || SystemTime::now().duration_since(data.last_cache_update.lock().await.clone()).unwrap_or_default().as_secs() > 30 {
        let client = &data.client;
        match get_peers(
            CKBNetworkType::from(query_params.network.clone()),
            query_params.offline_timeout,
            query_params.unknown_offline_timeout,
            &client,
        ).await {
            Ok(mut peers) => {
                if let Ok(mut d) = data.data.try_lock() {
                    d.clear();
                    d.append(&mut peers);
                    data.last_cache_update.lock().await.clone_from(&SystemTime::now());
                    let mut builder = HttpResponse::Ok();
                    builder.insert_header(CacheControl(vec![CacheDirective::MaxAge(45u32), CacheDirective::MinFresh(30u32)]));
                    builder.json(d.clone())
                } else {
                    let mut builder = HttpResponse::Ok();
                    builder.insert_header(CacheControl(vec![CacheDirective::MaxAge(45u32), CacheDirective::MinFresh(30u32)]));
                    builder.json(peers)
                }
            },
            Err(e) => {
                eprintln!("Error getting peers: {}", e);
                HttpResponse::InternalServerError().finish()
            }
        }
    } else {
        let mut builder = HttpResponse::Ok();
        builder.insert_header(CacheControl(vec![CacheDirective::MaxAge(45u32), CacheDirective::MinFresh(30u32)]));
        builder.json(data.data.lock().await.clone())
    }
}

async fn last_update_handler(data: Data<ServiceData>) -> impl Responder {
    let client = &data.client;
    match last_update(&client).await {
        Ok(status) => HttpResponse::Ok().json(status),
        Err(e) => {
            eprintln!("Error getting status: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

fn reachable_keys_to_peer_ids(keys: &Vec<String>) -> Vec<&str> {
    keys.into_iter().map(|key| key.rsplit('.').collect::<Vec<_>>()[1]).collect::<Vec<_>>()
}

fn keys_to_peer_ids(keys: &Vec<String>) -> Vec<&str> {
    keys.into_iter().map(|key| key.rsplit_once('.').map(|(_, part)| part).unwrap_or("")).collect::<Vec<_>>()
}

async fn last_update(client: &Data<Mutex<redis::aio::Connection>>) -> Result<ServiceStatus, redis::RedisError> {
    let mut client = client.lock().await;
    match client.get("service.last_update".to_string()).await {
        Ok(timestamp) => {
            Ok(ServiceStatus { last_update: timestamp})
            //Ok(ServiceStatus {  last_update: timestamp })
        },
        Err(err) => Err(err),
    }
}

async fn get_peers(network: CKBNetworkType, offline_min: u64, unknown_offline_min: u64, client: &Mutex<redis::aio::Connection>) -> Result<Vec<Peer>, redis::RedisError> {
    let mut client = client.lock().await;
    let keys: Vec<String> = match network {
        CKBNetworkType::Mirana => {
            client.keys::<String, Vec<String>>("network.peer.*.mirana".to_string()).await?
        },
        CKBNetworkType::Pudge => {
            client.keys::<String, Vec<String>>("network.peer.*.pudge".to_string()).await?
        },
        _ => { unreachable!() }
    };

    let peer_ids = reachable_keys_to_peer_ids(&keys);

    let online_keys: Vec<String> =  client.keys("peer.online.*").await?;
    let unknown_keys: Vec<String> = client.keys("peer.unknown.*").await?;
    let online_peers = keys_to_peer_ids(&online_keys);
    let unknown_peers = keys_to_peer_ids(&unknown_keys);
    
    let mut peers = Vec::new();

    for (index,peer_id) in peer_ids.into_iter().enumerate() {
        if !online_peers.contains(&peer_id) && !unknown_peers.contains(&peer_id) {
            continue
        }
        let version: String = if online_peers.contains(&peer_id) {
            let version_str = client.get(format!("peer.online.{}", peer_id)).await?;
            version_str
        } else {
            String::default()
        };

        let version_short = if !online_peers.contains(&peer_id) {
            "Unknown".to_string()
        } else {
            Regex::new(r"^(.*?)[^0-9.].*$")
                .unwrap()
                .captures(&version.clone())
                .unwrap()[1]
                .to_owned()
        };

        let country: Option<String> = client.get(format!("peer_info.{}.country", peer_id)).await.unwrap_or_default();
        let city: Option<String> = client.get(format!("peer_info.{}.city", peer_id)).await.unwrap_or_default();

        let (latitude, longitude) = match client.get::<String,String>(format!("peer_info.{}.pos", peer_id)).await {
            Ok(loc) => {
                let mut lat_lon = loc.split(',');
                // Parse each part to f64, providing a default if the value can't be parsed
                let latitude: Option::<f64> = lat_lon.next().and_then(|s| f64::from_str(s).ok());
                let longitude: Option::<f64> = lat_lon.next().and_then(|s| f64::from_str(s).ok());
                (latitude, longitude)
            },
            _ => (None, None),
        };

        let timestamp: u64 = client.get(format!("peer_info.{}.last_seen", peer_id)).await.unwrap_or_default();
        let last_seen = DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(timestamp)).into();

        peers.push(Peer {
            id: index as i32,
            version,
            version_short,
            last_seen: Some(last_seen),
            country,
            city,
            latitude,
            longitude,
            node_type: 0,
        });
    }

    Ok(peers)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("Marci")
        .arg(
            Arg::with_name("redis")
                .long("redis-url")
                .takes_value(true)
                .required(true)
                .default_value("redis://:CkBdIsCoVeRy@127.0.0.1")
                .help("The URL of the Redis"),
        )
        .arg(
            Arg::with_name("bind")
                .long("bind")
                .takes_value(true)
                .required(false)
                .default_value("0.0.0.0:1800")
                .help("The address to bind the server to"),
        )
        .get_matches();


    let bind = matches.value_of("bind").unwrap();
    let redis_url = matches.value_of("redis").unwrap();


    // redis context
    let redis_client = redis::Client::open(redis_url).expect("Error redis url!");
    let con = redis_client.get_tokio_connection().await.expect("Can not get redis context!");
    let client = Data::new(Mutex::new(con));
    // Start the HTTP server
    let app = HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .max_age(3600);
        App::new()
            .wrap(cors)
            .app_data(
                Data::new(
                    ServiceData {
                        client: client.clone(),
                        last_cache_update: Mutex::new(SystemTime::UNIX_EPOCH),
                        data: Mutex::new(Default::default()),
                    }
                )
            )
            .route("/peer", web::get().to(peer_handler))
            .route("/", web::get().to(peer_handler))
            .route("/health", web::get().to(last_update_handler))
    })
        .bind(bind)?;

    app.run().await
}

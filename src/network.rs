use ckb_discovery_types::{CKBNetworkType, EndpointInfo, NodeMetaInfo};
use p2p::multiaddr::Protocol;
use p2p::{multiaddr::Multiaddr, utils::multiaddr_to_socketaddr};
use std::net::IpAddr;

pub fn get_bootnodes(network: CKBNetworkType) -> Vec<Multiaddr> {
    let bootnodes = match network {
        CKBNetworkType::Mirana => [
            "/ip4/52.33.75.45/tcp/8124/p2p/QmPHzhEzekyjB14eQsnsVRxmymtYmAyntcfJgVYRiUq4Lb",
            "/ip4/47.110.15.57/tcp/8114/p2p/QmXS4Kbc9HEeykHUTJCm2tNmqghbvWyYpUp6BtE5b6VrAU",
            "/ip4/13.234.144.148/tcp/8114/p2p/QmbT7QimcrcD5k2znoJiWpxoESxang6z1Gy9wof1rT1LKR",
            "/ip4/104.208.105.55/tcp/8114/p2p/QmejugEABNzAofqRhci7HAipLFvoyYKRacd272jNtnQBTE",
            "/ip4/34.64.120.143/tcp/8114/p2p/QmejEJEbDcGGMp4D6WtftMMVLkR1ZuBfMgyLFDMJymkDt6",
            "/ip4/3.218.170.86/tcp/8114/p2p/QmShw2vtVt49wJagc1zGQXGS6LkQTcHxnEV3xs6y8MAmQN",
            "/ip4/35.236.107.161/tcp/8114/p2p/QmSRj57aa9sR2AiTvMyrEea8n1sEM1cDTrfb2VHVJxnGuu",
            "/ip4/23.101.191.12/tcp/8114/p2p/QmexvXVDiRt2FBGptgK4gBJusWyyTEEaHeuCAa35EPNkZS",
            "/ip4/13.37.172.80/tcp/8114/p2p/QmXJg4iKbQzMpLhX75RyDn89Mv7N2H8vLePBR7kgZf6hYk",
            "/ip4/34.118.49.255/tcp/8114/p2p/QmeCzzVmSAU5LNYAeXhdJj8TCq335aJMqUxcvZXERBWdgS",
            "/ip4/40.115.75.216/tcp/8114/p2p/QmW3P1WYtuz9hitqctKnRZua2deHXhNePNjvtc9Qjnwp4q",
        ]
        .to_vec(),
        CKBNetworkType::Pudge => [
            "/ip4/47.111.169.36/tcp/8111/p2p/QmNQ4jky6uVqLDrPU7snqxARuNGWNLgSrTnssbRuy3ij2W",
            "/ip4/35.176.207.239/tcp/8111/p2p/QmSJTsMsMGBjzv1oBNwQU36VhQRxc2WQpFoRu1ZifYKrjZ",
            "/ip4/18.136.60.221/tcp/8111/p2p/QmTt6HeNakL8Fpmevrhdna7J4NzEMf9pLchf1CXtmtSrwb",
            "/ip4/47.74.66.72/tcp/8111/p2p/QmPhgweKm2ciYq52LjtEDmKFqHxGcg2WQ8RLCayRRycanD",
            "/ip4/47.254.175.152/tcp/8111/p2p/QmXw7RsAR9bghvW4LrjrVBEwTMbdnpTEdWEtZSVQFpUgqU",
            "/ip4/47.245.29.58/tcp/8111/p2p/QmYWiwxHasuyou5ztw5uQkHbhm6gs6RB84sbgYxZNGHgaW",
            "/ip4/47.254.234.14/tcp/8111/p2p/QmfUJGvgXRTM12rSFScXy9GyD3ssuLVSpvscP5nngbhNbU",
            "/ip4/47.89.252.15/tcp/8111/p2p/QmRcUV32qumGrhkTXCWXrMhwGZLKZTmuFzLWacMpSZJ3n9",
            "/ip4/39.104.177.87/tcp/8111/p2p/QmQ27jbnww6deXQiv7SmYAUBPA1S3vGcqP9aRsXa4VaXEi",
            "/ip4/13.228.149.113/tcp/8111/p2p/QmQoTR39rBkpZVgLApDGDoFnJ2YDBS9hYeiib1Z6aoAdEf",
        ]
        .to_vec(),
        CKBNetworkType::Dev => {
            // Use local node
            ["/ip4/127.0.0.1/tcp/8114"].to_vec()
        }
    };
    bootnodes
        .as_slice()
        .iter()
        .map(|x| x.parse().unwrap())
        .collect()
}

pub fn addr_to_ip(addr: &Multiaddr) -> String {
    addr.iter()
        .find_map(|protocol| match protocol {
            Protocol::Ip4(ip4) => Some(ip4.to_string()),
            Protocol::Ip6(ip6) => ip6
                .to_ipv4()
                .map(|ip4| ip4.to_string())
                .or_else(|| Some(ip6.to_string())),
            Protocol::Dns4(dns4) => Some(dns4.to_string()),
            Protocol::Dns6(dns6) => Some(dns6.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| {
            let socket_addr = multiaddr_to_socketaddr(addr).unwrap();
            socket_addr.ip().to_string()
        })
}

pub fn addr_to_node_meta(addr: &Multiaddr, network: CKBNetworkType) -> NodeMetaInfo {
    let mut peer_id = String::new();
    let mut port = 0;
    let mut address = "127.0.0.1".parse().unwrap();
    for protocol in addr.iter() {
        match protocol {
            Protocol::Ip4(addr) => address = IpAddr::V4(addr),
            Protocol::Ip6(addr) => address = IpAddr::V6(addr),
            Protocol::P2P(p2p) => {
                peer_id =
                    String::from_utf8_lossy(bs58::encode(p2p).into_vec().as_slice()).to_string()
            }
            Protocol::Tcp(p) => port = p,
            _ => {}
        }
    }
    NodeMetaInfo {
        peer_id,
        addresses: vec![EndpointInfo { address, port }],
        network,
    }
}

pub fn addr_to_endpoint(addr: &Multiaddr) -> EndpointInfo {
    let mut port = 0;
    let mut address = "127.0.0.1".parse().unwrap();
    for protocol in addr.iter() {
        match protocol {
            Protocol::Ip4(addr) => address = IpAddr::V4(addr),
            Protocol::Ip6(addr) => address = IpAddr::V6(addr),
            Protocol::Tcp(p) => port = p,
            _ => {}
        }
    }
    EndpointInfo { address, port }
}

pub fn meta_info_to_addr(meta: &NodeMetaInfo) -> Vec<Result<Multiaddr, p2p::multiaddr::Error>> {
    meta.addresses
        .iter()
        .filter_map(|x| {
            if x.address.is_loopback() {
                None
            } else {
                match x.address {
                    IpAddr::V4(addr) => {
                        Some(format!("/ip4/{}/tcp/{}/p2p/{}", addr, x.port, meta.peer_id).parse())
                    }
                    IpAddr::V6(addr) => {
                        Some(format!("/ip6/{}/tcp/{}/p2p/{}", addr, x.port, meta.peer_id).parse())
                    }
                }
            }
        })
        .collect::<Vec<Result<Multiaddr, p2p::multiaddr::Error>>>()
}

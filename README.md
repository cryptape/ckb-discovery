# CKB-Discovery
This is a re-implementation of the CKB-Node-Probe project, Contains 3 module:

## Discovery

This is a dialer/probe module, used to dial/find reachable peers. It won't dial reachable peers directly, instead it will receive peer info from MQTT channel `peer/needs_dial`, and dial for that.
(Except bootnodes)


## Marci

This is the center that controls which peer should be called, which peer should be treated as online and so on.
Think it as a broadcaster and msg channel guider.

## Exposed

This is a query service provided for the frontend


## How to start

For the data center, run `docker-compose up -d`, it will:
1. start a MQTT service, expose at `1883`&`1884`
2. start a Redis server, expose at `6379`
3. start the expose server(query api service), expose at 1800
4. start marci service


If you want to run a dialer, run `docker build . -t ckb-discovery && docker run --name ckb-discovery -e MQTT_URL="mqtt://127.0.0.1:1883" -d ckb-discovery`

Remember to replace `127.0.0.1` with the data center ip.
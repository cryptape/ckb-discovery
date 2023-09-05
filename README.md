# CKB-Discovery
This is a re-implementation of the CKB-Node-Probe project, Contains 3 module:

## Discovery

This is a probe, used to dial/find reachable peers. It won't dial reachable peers directly, instead it will receive peer info from MQTT channel `peer/needs_dial`, and dial for that.
(Except bootnodes)


## Marci

This is the center that controls which peer should be called, which peer should be treated as online and so on.
Think it as a broadcaster and msg channel guider.

## Exposed

This is a query service provided for the frontend
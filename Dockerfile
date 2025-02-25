FROM rust:1.85.0 as build
WORKDIR /usr/src/ckb-discovery
LABEL author="Code Monad<codemonad@cryptape.com>"
COPY . .
RUN apt-get update && apt-get install libssl-dev build-essential cmake -y
RUN cargo build --release

FROM debian:12
RUN apt-get update && apt-get install libssl3 ca-certificates -y
RUN update-ca-certificates
WORKDIR /app/ckb-discovery
COPY --from=build /usr/src/ckb-discovery/target/release/ckb-discovery /app/ckb-discovery
#COPY --from=build /usr/src/marci/dist /app/marci/dist
ENV MQTT_URL="mqtt://mqtt:1883"
CMD ["/app/ckb-discovery/ckb-discovery"]

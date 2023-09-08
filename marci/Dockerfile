FROM rust:1.72.0 as build
WORKDIR /usr/src/ckb-discovery
LABEL author="Code Monad<codemonad@cryptape.com>"
COPY . .
RUN apt-get update && apt-get install libssl-dev build-essential cmake -y
RUN cd /usr/src/ckb-discovery/
RUN cargo build -p marci --release

FROM debian:12
RUN apt-get update && apt-get install libssl3 ca-certificates -y && update-ca-certificates
WORKDIR /app/marci
COPY --from=build /usr/src/ckb-discovery/target/release/marci /app/marci
ENV MQTT_URL="mqtt://mqtt:1883"
ENV REDIS_URL="redis://:CkBdIsCoVeRy@redis"
CMD ["/app/marci/marci"]
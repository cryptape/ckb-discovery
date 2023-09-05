FROM rust:1.72.0 as build
WORKDIR /usr/src/marci
LABEL author="Code Monad<codemonad@cryptape.com>"
COPY . .
RUN cargo build --release

FROM ubuntu:20.04

WORKDIR /app/marci
COPY --from=build /usr/src/marci/target/release/marci /app/marci
#COPY --from=build /usr/src/marci/dist /app/marci/dist
ENV MQTT_URL="mqtt://mqtt:1883"
ENV REDIS_URL="redis://:CkBdIsCoVeRy@redis"
CMD ["/app/marci/marci"]
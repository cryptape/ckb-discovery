FROM rust:1.85.0 as build
WORKDIR /usr/src/ckb-discovery
LABEL author="Code Monad<codemonad@cryptape.com>"
COPY . .
RUN cd /usr/src/ckb-discovery/exposed
RUN cargo build -p exposed --release

FROM debian:12
RUN apt-get update && apt-get install libssl3 -y
WORKDIR /app/exposed
COPY --from=build /usr/src/ckb-discovery/target/release/exposed /app/exposed
#COPY --from=build /usr/src/marci/dist /app/marci/dist
ENV REDIS_URL=redis://:CkBdIsCoVeRy@redis
ENV BIND=0.0.0.0:1800
CMD /app/exposed/exposed --redis-url $REDIS_URL --bind $BIND

FROM rust:1.72.0 as build
WORKDIR /usr/src/exposed
LABEL author="Code Monad<codemonad@cryptape.com>"
COPY . .
RUN cargo build --release

FROM ubuntu:20.04

WORKDIR /app/exposed
COPY --from=build /usr/src/exposed/target/release/exposed /app/exposed
#COPY --from=build /usr/src/marci/dist /app/marci/dist
ENV REDIS_URL="redis://:CkBdIsCoVeRy@redis"
ENV BIND="0.0.0.0:1800"
CMD ["/app/exposed/exposed --redis-url $DB_URL --bind $BIND"]
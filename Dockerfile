FROM rust:1-slim-buster as builder

WORKDIR /app

COPY . .

RUN cargo build --release

FROM debian:buster-slim

COPY --from=builder /app/target/release/pankkivahva /app/pankkivahva

CMD "/app/pankkivahva"
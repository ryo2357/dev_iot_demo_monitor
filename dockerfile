
# ビルド環境
FROM rust:1.75.0 AS build
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

# 本番環境
FROM debian AS production
WORKDIR /usr/local/bin
RUN apt-get update && apt install -y openssl
COPY --from=build /usr/src/app/target/release/iot_gateway ./
COPY .env .
CMD ["./iot_gateway"]


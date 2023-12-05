
# ビルド環境
FROM rust:1.73.0 AS build
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

# 本番環境
FROM debian AS production
WORKDIR /usr/local/bin
COPY --from=build /usr/src/app/target/release/iot_gateway ./
CMD ["./iot_gateway"]


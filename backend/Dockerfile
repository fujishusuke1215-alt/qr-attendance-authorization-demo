FROM rust:1.100-slim-bookworm AS build
WORKDIR /service
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY migrations ./migrations
COPY fixtures ./fixtures
COPY src ./src
RUN cargo build --locked --release --bin attendance-service

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home service
WORKDIR /service
COPY --from=build /service/target/release/attendance-service /usr/local/bin/attendance-service
USER 10001:10001
ENV API_HOST=0.0.0.0 API_PORT=8080 APP_ENV=production AUTH_MODE=entra
EXPOSE 8080
ENTRYPOINT ["attendance-service"]

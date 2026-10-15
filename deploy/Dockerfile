# syntax=docker/dockerfile:1
FROM rust:1.94-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY server ./server
COPY semantic ./semantic
RUN cargo build -p sql-mcp --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/sql-mcp /usr/local/bin/sql-mcp
ENV SQL_MCP_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["sql-mcp"]

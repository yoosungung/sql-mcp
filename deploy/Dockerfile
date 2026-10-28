# syntax=docker/dockerfile:1
FROM node:22-bookworm AS ui
WORKDIR /src/semantic-ui
COPY semantic-ui/package.json semantic-ui/package-lock.json ./
RUN npm ci
COPY semantic-ui/index.html semantic-ui/tsconfig.json semantic-ui/vite.config.ts ./
COPY semantic-ui/src ./src
RUN npm run build

FROM rust:1.94-bookworm AS build
WORKDIR /src
ENV SQL_MCP_SKIP_UI_BUILD=1
COPY Cargo.toml Cargo.lock ./
COPY server ./server
COPY semantic ./semantic
COPY skills ./skills
RUN cargo build -p sql-mcp --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/sql-mcp /usr/local/bin/sql-mcp
COPY --from=ui /src/semantic-ui/dist /app/semantic-ui/dist
ENV SQL_MCP_LISTEN=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["sql-mcp"]

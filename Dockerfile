# File: Builds the Rust services and TypeScript dashboard into a minimal non-root runtime image.
FROM node:24.10.0-bookworm-slim AS web-build
WORKDIR /workspace/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --ignore-scripts
COPY web/ ./
RUN npm run build

FROM rust:1.98.1-bookworm AS rust-build
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release --locked --bins

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 orchestrator
WORKDIR /app
COPY --from=rust-build /workspace/target/release/orchestratord /usr/local/bin/orchestratord
COPY --from=rust-build /workspace/target/release/orchestrator-worker /usr/local/bin/orchestrator-worker
COPY --from=rust-build /workspace/target/release/orchestratorctl /usr/local/bin/orchestratorctl
COPY --from=web-build /workspace/web/dist ./web/public
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["orchestratord"]


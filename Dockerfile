FROM node:24-bookworm-slim@sha256:d6aa754f16b3197301076f047b5def2f02ea1dbbc2ca920407d46d7ec7f87b20 AS web-build
WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

FROM rust:1.96-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663 AS rust-build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
COPY --from=web-build /app/web/dist ./web/dist
RUN cargo build --locked --release --bin codexpt

FROM debian:bookworm-slim@sha256:96e378d7e6531ac9a15ad505478fcc2e69f371b10f5cdf87857c4b8188404716 AS runtime
RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home --home-dir /nonexistent codexpt \
    && mkdir /data \
    && chown codexpt:codexpt /data

COPY --from=rust-build /app/target/release/codexpt /usr/local/bin/codexpt
COPY scripts/container-entrypoint.sh /usr/local/bin/container-entrypoint
RUN chmod 0755 /usr/local/bin/container-entrypoint

ENV CODEXPT_DB_PATH=/data/codexpt.sqlite3
ENV PORT=4173
EXPOSE 4173
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/container-entrypoint"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD sh -c 'test -n "$CODEXPT_API_HOST" && curl --fail --silent --show-error -H "Host: $CODEXPT_API_HOST" "http://127.0.0.1:${PORT:-4173}/api/health/ready" >/dev/null || exit 1'

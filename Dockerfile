# Squirrel: one image for `serve` and `worker` (see deploy/README.md).
#
#   docker buildx build --platform linux/amd64 -t squirrel:latest --load .
#
# Everything the server needs at run time is pinned and verified while building: pdfium,
# ONNX Runtime and the ML models (scripts/fetch-*.sh check each SHA-256), and the GeoNames
# snapshot from data/geodata. Nothing is downloaded at run time.

# Cargo features of the `squirrel` binary. HEIC needs the system libheif (installed below).
ARG FEATURES=squirrel/heic

# --- Web UI: embedded into the binary by rust-embed (crates/squirrel-api/src/spa.rs) ---
FROM node:22-trixie-slim AS web
WORKDIR /src/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml web/.nvmrc ./
RUN --mount=type=cache,id=squirrel-pnpm,target=/root/.local/share/pnpm/store \
    pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# --- Server binary ---
FROM rust:1.94-trixie AS rust
ARG FEATURES
RUN apt-get update \
 && apt-get install -y --no-install-recommends libheif-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
COPY --from=web /src/web/build web/build
RUN --mount=type=cache,id=squirrel-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=squirrel-target,target=/src/target \
    cargo build --release --locked -p squirrel --features "${FEATURES}" \
 && install -D -m 0755 target/release/squirrel /out/squirrel

# --- Pinned runtime assets (each script verifies its SHA-256 pins) ---
FROM rust:1.94-trixie AS assets
RUN apt-get update \
 && apt-get install -y --no-install-recommends unzip \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY scripts/fetch-pdfium.sh scripts/fetch-onnxruntime.sh scripts/fetch-models.sh scripts/
RUN scripts/fetch-pdfium.sh /opt/squirrel/pdfium \
 && SQUIRREL_ORT_CACHE=/tmp/ort-cache scripts/fetch-onnxruntime.sh /opt/squirrel/onnxruntime \
 && scripts/fetch-models.sh /opt/squirrel/models \
 && rm -rf /tmp/ort-cache
# The GeoNames snapshot, decompressed and checked against the pin compiled into the binary.
COPY data/geodata/ data/geodata/
COPY --from=rust /out/squirrel /usr/local/bin/squirrel
RUN set -eu; \
    line="$(squirrel admin geodata pinned)"; hash="${line%%  *}"; name="${line#*  }"; \
    mkdir -p /opt/squirrel/geo; \
    gzip -dc "data/geodata/${name}.gz" > "/opt/squirrel/geo/${name}"; \
    echo "${hash}  /opt/squirrel/geo/${name}" | sha256sum -c -; \
    chmod -R a=rX,u+w /opt/squirrel

# --- Runtime ---
FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini libheif1 libheif-plugin-libde265 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin squirrel \
 && install -d -o squirrel -g squirrel -m 0750 /data
# Models, ONNX Runtime and pdfium stay root-owned and read-only to the service user: the
# children verify them before loading, and nothing may swap them in between (README).
COPY --from=assets /opt/squirrel /opt/squirrel
COPY --from=rust /out/squirrel /usr/local/bin/squirrel
COPY --chmod=0755 deploy/entrypoint.sh /usr/local/bin/squirrel-entrypoint
ENV SQUIRREL_DATA_DIR=/data \
    SQUIRREL_BIND=0.0.0.0:8080 \
    SQUIRREL_LOG_JSON=true \
    SQUIRREL_MODELS_DIR=/opt/squirrel/models \
    SQUIRREL_ORT_DYLIB=/opt/squirrel/onnxruntime/lib/libonnxruntime.so \
    SQUIRREL_PDFIUM_DIR=/opt/squirrel/pdfium/lib
USER squirrel
WORKDIR /data
EXPOSE 8080
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/squirrel-entrypoint"]
CMD ["serve"]

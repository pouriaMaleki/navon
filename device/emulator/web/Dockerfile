# Multi-stage image that serves the companion-web app at `/` and the emulator-web
# app at `/emulator/` from a single nginx container. The service name stays
# `emulator-web` in compose.yaml so the existing edge routing (e.g. navon.bike)
# keeps working without any changes upstream.

# ------ Stage 1: build the render-core wasm package the emulator needs. ------
FROM rust:1.88-bookworm AS wasm-builder

RUN apt-get update \
  && apt-get install -y --no-install-recommends curl pkg-config libssl-dev ca-certificates \
  && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown \
  && curl -sSf https://rustwasm.github.io/wasm-pack/installer/init.sh | sh

WORKDIR /work

# Surgical copies — pulling only the Rust workspace bits wasm-pack needs
# instead of `COPY . .`. Without this the build context dragged in
# companion-apps/web/node_modules (~200 MB), companion-apps/android/build,
# Xcode DerivedData, etc., even though wasm-pack only consumes ~2 MB of
# workspace source. Layers are also cache-friendly per-crate now.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo ./.cargo

# Crates render-core-wasm pulls (transitively) at build time.
COPY device/core/render-core ./render-core
COPY device/core/render-core-wasm ./render-core-wasm
COPY device/core/runtime-core ./runtime-core
COPY device/core/map-runtime ./map-runtime
COPY device/core/route-import-gpx ./route-import-gpx

# `map-runtime` embeds a city map via `include_bytes!("../../../../data/map-data/...")`,
# so the directory must be present at compile time even when the file is
# the empty placeholder shipped in the repo.
COPY data/map-data ./map-data

# Workspace members not in the wasm dependency graph but required for
# `cargo` to parse the workspace manifest. Their sources are tiny and
# never compiled here (wasm-pack targets render-core-wasm specifically).
COPY device/firmware ./firmware
COPY data/parity-fixtures ./parity-fixtures
COPY tools/map-vector-cli ./map-vector-cli
COPY tools/xtask ./xtask

RUN wasm-pack build device/core/render-core-wasm --target web --out-dir /tmp/wasm-pkg --out-name render_core_wasm

# ------ Stage 2: build the emulator web app under a /emulator/ base path. ------
FROM node:20-bookworm-slim AS emulator-builder

WORKDIR /app

COPY device/emulator/web/package.json device/emulator/web/package-lock.json ./
RUN npm ci

COPY device/emulator/web/index.html ./
COPY device/emulator/web/web-fullscreen.html ./
COPY device/emulator/web/tsconfig.json ./
COPY device/emulator/web/vite.config.ts ./
COPY device/emulator/web/src ./src
COPY --from=wasm-builder /tmp/wasm-pkg ./wasm-pkg

ENV VITE_BASE_PATH=/emulator/
RUN npm run build

# ------ Stage 3: build the companion web app at the site root. ------
FROM node:20-bookworm-slim AS companion-builder

WORKDIR /app

COPY companion-apps/web/package.json companion-apps/web/package-lock.json ./
RUN npm ci

COPY companion-apps/web/index.html ./
COPY companion-apps/web/tsconfig.json ./
COPY companion-apps/web/vite.config.ts ./
COPY companion-apps/web/biome.json ./
COPY companion-apps/web/src ./src

RUN npm run build

# ------ Stage 4: nginx serving both apps from one document root. ------
FROM nginx:1.27-alpine AS runtime

COPY device/emulator/web/nginx.conf /etc/nginx/conf.d/default.conf

COPY --from=companion-builder /app/dist/ /usr/share/nginx/html/
COPY --from=emulator-builder /app/dist/ /usr/share/nginx/html/emulator/

EXPOSE 4173

CMD ["nginx", "-g", "daemon off;"]

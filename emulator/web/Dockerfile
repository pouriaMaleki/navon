# Multi-stage image that serves the companion-web app at `/` and the emulator-web
# app at `/emulator/` from a single nginx container. The service name stays
# `emulator-web` in compose.yaml so the existing edge routing (e.g. map.fiksu.me)
# keeps working without any changes upstream.

# ------ Stage 1: build the render-core wasm package the emulator needs. ------
FROM rust:1.88-bookworm AS wasm-builder

RUN apt-get update \
  && apt-get install -y --no-install-recommends curl pkg-config libssl-dev ca-certificates \
  && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown \
  && curl -sSf https://rustwasm.github.io/wasm-pack/installer/init.sh | sh

WORKDIR /work
COPY . .

RUN wasm-pack build render-core-wasm --target web --out-dir /tmp/wasm-pkg --out-name render_core_wasm

# ------ Stage 2: build the emulator web app under a /emulator/ base path. ------
FROM node:20-bookworm-slim AS emulator-builder

WORKDIR /app

COPY emulator/web/package.json emulator/web/package-lock.json ./
RUN npm ci

COPY emulator/web/index.html ./
COPY emulator/web/web-fullscreen.html ./
COPY emulator/web/tsconfig.json ./
COPY emulator/web/vite.config.ts ./
COPY emulator/web/src ./src
COPY --from=wasm-builder /tmp/wasm-pkg ./wasm-pkg

ENV VITE_BASE_PATH=/emulator/
RUN npm run build

# ------ Stage 3: build the companion web app at the site root. ------
FROM node:20-bookworm-slim AS companion-builder

WORKDIR /app

COPY companion-web/package.json companion-web/package-lock.json ./
RUN npm ci

COPY companion-web/index.html ./
COPY companion-web/tsconfig.json ./
COPY companion-web/vite.config.ts ./
COPY companion-web/biome.json ./
COPY companion-web/src ./src

RUN npm run build

# ------ Stage 4: nginx serving both apps from one document root. ------
FROM nginx:1.27-alpine AS runtime

COPY emulator/web/nginx.conf /etc/nginx/conf.d/default.conf

COPY --from=companion-builder /app/dist/ /usr/share/nginx/html/
COPY --from=emulator-builder /app/dist/ /usr/share/nginx/html/emulator/

EXPOSE 4173

CMD ["nginx", "-g", "daemon off;"]

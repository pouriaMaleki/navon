FROM rust:1.85-bookworm AS wasm-builder

RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
  && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown \
  && cargo install wasm-pack --locked

WORKDIR /work
COPY . .

RUN wasm-pack build render-core-wasm --target web --out-dir /tmp/wasm-pkg --out-name render_core_wasm

FROM node:20-bookworm-slim

WORKDIR /app

RUN npm install --global http-server

COPY emulator/web/package.json emulator/web/package-lock.json ./
RUN npm ci

COPY emulator/web/index.html ./
COPY emulator/web/fullscreen.html ./
COPY emulator/web/web-fullscreen.html ./
COPY emulator/web/tsconfig.json ./
COPY emulator/web/vite.config.ts ./
COPY emulator/web/src ./src
COPY --from=wasm-builder /tmp/wasm-pkg ./wasm-pkg

RUN npm run build

EXPOSE 4173

CMD ["http-server", "dist", "-a", "0.0.0.0", "-p", "4173", "-c-1"]

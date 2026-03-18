FROM rust:1.86-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    libasound2-dev \
    libgl1-mesa-dev \
    libglu1-mesa-dev \
    libudev-dev \
    libwayland-dev \
    libx11-dev \
    libxcursor-dev \
    libxinerama-dev \
    libxkbcommon-dev \
    libxrandr-dev \
    nodejs \
    npm \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY jarvis_rs/Cargo.toml jarvis_rs/Cargo.toml
COPY browser_sidecar/package.json browser_sidecar/package-lock.json browser_sidecar/
RUN mkdir -p jarvis_rs/src && printf "fn main() {}\n" > jarvis_rs/src/main.rs
RUN printf "pub fn placeholder() {}\n" > jarvis_rs/src/lib.rs
RUN cargo build --release --bin agent_core_server || true

COPY . .
RUN npm ci --prefix browser_sidecar
RUN cargo build --release --bin agent_core_server --bin scenario_runner

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libasound2 \
    libgl1 \
    libglu1-mesa \
    libudev1 \
    libwayland-client0 \
    libx11-6 \
    libxcursor1 \
    libxinerama1 \
    libxkbcommon0 \
    libxrandr2 \
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/agent_core_server /usr/local/bin/agent_core_server
COPY --from=builder /app/target/release/scenario_runner /usr/local/bin/scenario_runner
COPY --from=builder /app/browser_sidecar /app/browser_sidecar

ENV JARVIS_AGENT_CORE_BIND=0.0.0.0:8788

EXPOSE 8788

HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD curl -fsS http://127.0.0.1:8788/health || exit 1

CMD ["/usr/local/bin/agent_core_server"]

# Build stage
FROM rust:latest AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifest files first to cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    # We need to create a dummy lib.rs because the Cargo.toml defines a lib
    touch src/lib.rs && \
    cargo build --release --features api || true

# Remove dummy files
RUN rm -rf src

# Copy actual source code
COPY . .

# Build the application
RUN cargo build --release --features api

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy config if exists
COPY Config.toml ./

# Copy binary from builder
COPY --from=builder /app/target/release/hyperchess .

EXPOSE 3000

CMD ["./hyperchess"]

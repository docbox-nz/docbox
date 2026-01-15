#  Builder part
FROM rust:1.92.0-slim-bullseye AS builder

# Add rust target and install deps
RUN update-ca-certificates

WORKDIR /app

# Dependency precachng
COPY Cargo.toml .
COPY Cargo.lock .

# Copy crate cargo manifests
COPY packages/docbox/Cargo.toml packages/docbox/Cargo.toml
COPY packages/docbox-core/Cargo.toml packages/docbox-core/Cargo.toml
COPY packages/docbox-database/Cargo.toml packages/docbox-database/Cargo.toml
COPY packages/docbox-web-scraper/Cargo.toml packages/docbox-web-scraper/Cargo.toml
COPY packages/docbox-search/Cargo.toml packages/docbox-search/Cargo.toml
COPY packages/docbox-secrets/Cargo.toml packages/docbox-secrets/Cargo.toml
COPY packages/docbox-storage/Cargo.toml packages/docbox-storage/Cargo.toml
COPY packages/docbox-management/Cargo.toml packages/docbox-management/Cargo.toml
COPY packages/docbox-processing/Cargo.toml packages/docbox-processing/Cargo.toml
COPY packages/docbox-http/Cargo.toml packages/docbox-http/Cargo.toml

# Create empty entrypoints
RUN mkdir packages/docbox/src && echo "fn main() {}" >packages/docbox/src/main.rs
RUN mkdir packages/docbox-core/src && echo "//placeholder" >packages/docbox-core/src/lib.rs
RUN mkdir packages/docbox-database/src && echo "//placeholder" >packages/docbox-database/src/lib.rs
RUN mkdir packages/docbox-web-scraper/src && echo "//placeholder" >packages/docbox-web-scraper/src/lib.rs
RUN mkdir packages/docbox-search/src && echo "//placeholder" >packages/docbox-search/src/lib.rs
RUN mkdir packages/docbox-secrets/src && echo "//placeholder" >packages/docbox-secrets/src/lib.rs
RUN mkdir packages/docbox-storage/src && echo "//placeholder" >packages/docbox-storage/src/lib.rs
RUN mkdir packages/docbox-management/src && echo "//placeholder" >packages/docbox-management/src/lib.rs
RUN mkdir packages/docbox-processing/src && echo "//placeholder" >packages/docbox-processing/src/lib.rs
RUN mkdir packages/docbox-http/src && echo "//placeholder" >packages/docbox-http/src/lib.rs

# Run a build to download dependencies
RUN cargo build -p docbox --release

COPY packages packages

RUN touch packages/docbox/src/main.rs
RUN touch packages/docbox-core/src/lib.rs
RUN touch packages/docbox-database/src/lib.rs
RUN touch packages/docbox-web-scraper/src/lib.rs
RUN touch packages/docbox-search/src/lib.rs
RUN touch packages/docbox-secrets/src/lib.rs
RUN touch packages/docbox-storage/src/lib.rs
RUN touch packages/docbox-management/src/lib.rs
RUN touch packages/docbox-processing/src/lib.rs
RUN touch packages/docbox-http/src/lib.rs

RUN cargo build -p docbox --release

# ----------------------------------------
# Runner part
# ----------------------------------------
# Runner part
FROM debian:bullseye-slim AS runner

# Set environment variables to avoid interaction during installation
ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /app

# Install necessary tools
RUN apt-get update && apt-get install -y poppler-utils && apt-get clean

# Copy the built binary
COPY --from=builder /app/target/release/docbox ./

EXPOSE 8080

CMD ["/app/docbox"]

#  Builder part
FROM rust:1.87.0-slim

# Add rust target and install deps
RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools musl-dev
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
COPY packages/docbox-management/Cargo.toml packages/docbox-management/Cargo.toml

# Create empty entrypoints
RUN mkdir packages/docbox/src && echo "fn main() {}" >packages/docbox/src/main.rs
RUN mkdir packages/docbox-core/src && echo "//placeholder" >packages/docbox-core/src/lib.rs
RUN mkdir packages/docbox-database/src && echo "//placeholder" >packages/docbox-database/src/lib.rs
RUN mkdir packages/docbox-web-scraper/src && echo "//placeholder" >packages/docbox-web-scraper/src/lib.rs
RUN mkdir packages/docbox-search/src && echo "//placeholder" >packages/docbox-search/src/lib.rs
RUN mkdir packages/docbox-secrets/src && echo "//placeholder" >packages/docbox-secrets/src/lib.rs
RUN mkdir packages/docbox-management/src && echo "//placeholder" >packages/docbox-management/src/lib.rs

# Run a build to download dependencies
RUN cargo build -p docbox --target x86_64-unknown-linux-musl --release

COPY packages packages

RUN touch packages/docbox/src/main.rs
RUN touch packages/docbox-core/src/lib.rs
RUN touch packages/docbox-database/src/lib.rs
RUN touch packages/docbox-web-scraper/src/lib.rs
RUN touch packages/docbox-search/src/lib.rs
RUN touch packages/docbox-secrets/src/lib.rs
RUN touch packages/docbox-management/src/lib.rs

RUN cargo build -p docbox --target x86_64-unknown-linux-musl --release

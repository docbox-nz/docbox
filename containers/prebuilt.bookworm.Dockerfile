FROM debian:bookworm-slim

# Github release version
ARG GITHUB_RELEASE_VERSION

# Docker image arch
ARG TARGETARCH

# Setup working directory
WORKDIR /app

# Install necessary tools
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl ca-certificates poppler-utils && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Determine binary based on arch
RUN if [ "$TARGETARCH" = "amd64" ]; then \
    BINARY="docbox-x86_64-linux-gnu"; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
    BINARY="docbox-aarch64-linux-gnu"; \
    else \
    echo "Unsupported architecture: $TARGETARCH" && exit 1; \
    fi && \
    # Download docbox binary
    curl -L -o docbox https://github.com/docbox-nz/docbox/releases/download/${GITHUB_RELEASE_VERSION}/$BINARY && \
    # Make binary executable
    chmod +x docbox

EXPOSE 8080

CMD ["/app/docbox"]

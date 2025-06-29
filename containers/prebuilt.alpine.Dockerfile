FROM alpine

# Github release version
ARG GITHUB_RELEASE_VERSION

# Docker image arch
ARG TARGETARCH

# Setup working directory
WORKDIR /app

# Install necessary tools
RUN apk add --no-cache curl ca-certificates poppler-utils

# Determine binary based on arch
RUN if [ "$TARGETARCH" = "amd64" ]; then \
    BINARY="docbox-x86_64-linux-musl"; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
    BINARY="docbox-aarch64-linux-musl"; \
    else \
    echo "Unsupported architecture: $TARGETARCH" && exit 1; \
    fi && \
    # Download docbox binary
    curl -L -o docbox https://github.com/docbox-nz/docbox/releases/download/${GITHUB_RELEASE_VERSION}/$BINARY && \
    # Make binary executable
    chmod +x docbox

EXPOSE 8080

CMD ["/app/docbox"]

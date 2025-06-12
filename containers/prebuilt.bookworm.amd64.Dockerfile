FROM debian:bookworm-slim

# Setup working directory
WORKDIR /app

# Install necessary tools
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl ca-certificates poppler-utils && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Download docbox binary
RUN curl -L -o docbox https://github.com/docbox-nz/docbox/releases/download/0.1.0/docbox-x86_64-linux-gnu && \
    chmod +x docbox

EXPOSE 8080

CMD ["/app/docbox"]

FROM alpine

# Setup working directory
WORKDIR /app

# Install necessary tools
RUN apk add --no-cache curl ca-certificates poppler-utils

# Download docbox binary
RUN curl -L -o docbox https://github.com/docbox-nz/docbox/releases/download/0.1.0/docbox-aarch64-linux-musl && \
    chmod +x docbox

EXPOSE 8080

CMD ["/app/docbox"]

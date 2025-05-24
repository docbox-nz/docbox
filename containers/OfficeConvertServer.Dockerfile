# IF THIS CONTAINER BUILD IS FAILING ON WINDOWS ENSURE YOU ADD
# A DNS ENTRY TO YOUR DOCKER ENGINE CONFIGURATION
# Docker Desktop > Settings > Docker Engine
# Add following line:
# "dns": ["8.8.8.8"]
# And restart docker desktop

FROM debian:bookworm-slim 

# Set environment variables to avoid interaction during installation
ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /app

RUN echo 'deb http://deb.debian.org/debian/ bookworm main' > /etc/apt/sources.list.d/debian.list

# Bookworm backports for latest available LibreOffice version
RUN echo 'deb http://deb.debian.org/debian bookworm-backports main' > /etc/apt/sources.list.d/bookworm-backports.list

# Install dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl ca-certificates

# Install stable libreoffice from backports
RUN apt-get install -t bookworm-backports -y libreoffice

# Cleanup
RUN apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Download server executable
RUN curl -LJ -o office-convert-server https://github.com/jacobtread/office-convert-server/releases/download/v0.0.3/office-convert-server?v=1

RUN chmod +x /app/office-convert-server


ENV LIBREOFFICE_SDK_PATH=/usr/lib/libreoffice/program
ENV SERVER_ADDRESS=0.0.0.0:3000 

EXPOSE 8080

CMD ["/app/office-convert-server"]
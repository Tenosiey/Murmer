FROM rust:1-bullseye

# Install Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get update && apt-get install -y nodejs libwebkit2gtk-4.1-dev build-essential \
    libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

CMD ["bash"]

# Start with a pre-configured Ubuntu 18.04 image.
FROM ubuntu

RUN apt-get update -y

WORKDIR /root
# Copy the Fulcrum binary and config file.
COPY bin/Fulcrum Fulcrum
COPY regtest.conf regtest.conf
COPY entrypoint.sh entrypoint.sh

# Copy SSL certificates
COPY certs/fulcrum.crt fulcrum.crt
COPY certs/fulcrum.key fulcrum.key

RUN mkdir -p /data/fulcrum

ENV BITCOIND_HOST="bitcoin"
ENV BITCOIND_PORT="18443"
ENV DATADIR="/data/fulcrum"

# Start Fulcrum server.
CMD ["sh", "./entrypoint.sh"]
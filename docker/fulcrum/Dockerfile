# Start with a pre-configured Ubuntu 18.04 image.
FROM ubuntu

RUN apt-get update -y

WORKDIR /root
# Copy the Fulcrum binary and config file.
COPY bin/Fulcrum Fulcrum
COPY regtest.conf regtest.conf

# Copy SSL certificates
COPY certs/fulcrum.crt fulcrum.crt
COPY certs/fulcrum.key fulcrum.key

# Used to hold the Fulcrum database.
RUN mkdir blockchain

# Start Fulcrum server.
CMD ["./Fulcrum", "regtest.conf"]

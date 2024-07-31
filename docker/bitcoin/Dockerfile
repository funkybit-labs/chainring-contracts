FROM ubuntu

# copy scripts
COPY bitcoin /bitcoin

RUN apt update -y && apt install -y wget
RUN wget --directory-prefix=bitcoin https://bitcoincore.org/bin/bitcoin-core-27.1/bitcoin-27.1-x86_64-linux-gnu.tar.gz

# expose ports for bitcoind on local, testnet, and mainnet
EXPOSE 18443 18444 18445

ENV PATH="${PATH}:/bitcoin/bitcoin-27.1/bin"
ENV FAUCET_ADDRESS=bcrt1q3nyukkpkg6yj0y5tj6nj80dh67m30p963mzxy7

CMD ["sh", "/bitcoin/entrypoint.sh"]

FROM debian:bookworm-slim
RUN apt-get update

RUN apt-get install -qq --no-install-recommends curl jq

COPY init.sh /init.sh

ENTRYPOINT ["/init.sh"]
########################################################################

FROM ubuntu

COPY entrypoint.sh /

RUN apt update -y && apt install -y wget
RUN wget https://github.com/ordinals/ord/releases/download/0.21.3/ord-0.21.3-x86_64-unknown-linux-gnu.tar.gz

EXPOSE 7080

CMD ["sh", "/entrypoint.sh"]

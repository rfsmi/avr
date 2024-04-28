FROM ubuntu:20.04
RUN sed -i \
    's/htt[p|ps]:\/\/archive.ubuntu.com\/ubuntu\//mirror:\/\/mirrors.ubuntu.com\/mirrors.txt/g' \
    /etc/apt/sources.list
RUN apt update
RUN DEBIAN_FRONTEND=noninteractive apt install -y \
    clang \
    avr-libc \
    gcc-avr \
    pkg-config \
    libudev-dev \
    build-essential \
    git \
    curl
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
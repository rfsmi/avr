FROM ubuntu:22.04
RUN apt update
RUN apt install -y \
    usbutils \
    avrdude \
    avr-libc \
    gcc-avr \
    pkg-config \
    libudev-dev \
    build-essential \
    git \
    curl
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
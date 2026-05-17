FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        debhelper \
        devscripts \
        dh-cargo \
        dput \
        equivs \
        git \
        gnupg \
        lintian \
        pkg-config \
        software-properties-common \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable

ENV PATH="/root/.cargo/bin:${PATH}"

RUN cargo install cargo-deb

WORKDIR /work

CMD ["/bin/bash"]

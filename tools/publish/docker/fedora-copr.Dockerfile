FROM fedora:44

RUN dnf install -y \
    cargo \
    copr-cli \
    gcc \
    git \
    openssl-devel \
    pkgconf-pkg-config \
    rpm-build \
    rpmdevtools \
    rust \
    && dnf clean all

WORKDIR /work

CMD ["/bin/bash"]

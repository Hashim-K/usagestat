FROM archlinux:base-devel

RUN pacman -Syu --noconfirm \
    git \
    namcap \
    openssh \
    rust \
    && pacman -Scc --noconfirm

RUN useradd -m -G wheel builder \
    && printf '%%wheel ALL=(ALL:ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/wheel \
    && chmod 0440 /etc/sudoers.d/wheel

USER builder
WORKDIR /work

CMD ["/bin/bash"]

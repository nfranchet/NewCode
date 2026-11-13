FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

RUN rustup update stable && \
    rustup default stable && \
    rustup component add --toolchain stable llvm-tools-preview rustfmt && \
    sudo apt update && \
    sudo apt install -y tig zsh-antigen curl ca-certificates unzip fonts-dejavu-core python3 xz-utils && \
    cargo +stable install --locked grcov && \
    chown -R vscode:rustlang /usr/local/cargo

# The Compose build context is .devcontainer; pin the same toolchain as CI.
ARG ZIG_VERSION=0.15.2
RUN architecture="$(uname -m)" && \
    curl -fsSL "https://ziglang.org/download/${ZIG_VERSION}/zig-${architecture}-linux-${ZIG_VERSION}.tar.xz" -o /tmp/zig.tar.xz && \
    mkdir -p /opt/zig && tar -xJf /tmp/zig.tar.xz --strip-components=1 -C /opt/zig && \
    rm /tmp/zig.tar.xz
ENV PATH=/opt/zig:${PATH}

COPY ./vscode/files/toto.txt /tmp/toto.txt

USER vscode
ENV PATH=/home/vscode/.local/bin:/home/vscode/.opencode/bin:${PATH}
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN export HOME=/home/vscode && curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_NON_INTERACTIVE=1 sh
RUN export HOME=/home/vscode && curl -fsSL https://opencode.ai/install | bash
RUN codex --version && opencode --version
USER root

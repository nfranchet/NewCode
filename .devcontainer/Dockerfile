FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

RUN rustup update stable && \
    rustup default stable && \
    rustup component add --toolchain stable llvm-tools-preview && \
    sudo apt update && \
    sudo apt install -y tig zsh-antigen curl ca-certificates unzip && \
    cargo +stable install --locked grcov

USER vscode
ENV PATH=/home/vscode/.local/bin:/home/vscode/.opencode/bin:${PATH}
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN export HOME=/home/vscode && curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_NON_INTERACTIVE=1 sh
RUN export HOME=/home/vscode && curl -fsSL https://opencode.ai/install | bash
RUN codex --version && opencode --version
USER root

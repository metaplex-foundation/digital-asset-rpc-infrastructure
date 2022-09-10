FROM node:18
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN sh -c "$(curl -sSfL https://release.solana.com/v1.10.31/install)"
ENV PATH="/root/.cargo/bin:/root/.local/share/solana/install/active_release/bin:${PATH}"
RUN cargo install anchor-cli

COPY ./contracts/package.json .
RUN yarn

COPY ./contracts /rust/
RUN anchor build
CMD yarn run ts-node tests/bubblegum-test-rpc-fast.ts
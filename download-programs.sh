#!/usr/bin/env bash

CWD=$(pwd)

mkdir programs
mkdir metaplex_program_library || true
curl -LkSs https://api.github.com/repos/metaplex-foundation/metaplex-program-library/tarball | tar -xz --strip-components=1 -C ./metaplex_program_library

pushd metaplex_program_library/token-metadata/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/mpl_token_metadata.so $CWD/programs/metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s.so
popd

pushd metaplex_program_library/bubblegum/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/mpl_bubblegum.so $CWD/programs/BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY.so
popd

mkdir solana_program_library || true
curl -LkSs https://api.github.com/repos/solana-labs/solana-program-library/tarball | tar -xz --strip-components=1 -C ./solana_program_library
tar -zxf -C /solana_program_library solana-program-library.tar.gz
pushd solana_program_library/account-compression/programs/account-compression
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_account_compression.so $CWD/programs/GRoLLzvxpxxu2PGNJMMeZPyMxjAUH9pKqxGXV9DGiceU.so
popd

pushd solana_program_library/account-compression/programs/wrapper
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_noop.so $CWD/programs/WRAPYChf58WFCnyjXKJHtrPgzKXgHp6MD9aVDqJBbGh.so
popd

pushd solana_program_library
  rm -rf Cargo.toml
popd

pushd solana_program_library/associated-token-account/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_associated_token_account.so $CWD/programs/ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL.so
popd

pushd solana_program_library/token/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_token.so $CWD/programs/TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA.so
popd

pushd solana_program_library/token/program-2022
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_token_2022.so $CWD/programs/TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb.so
popd

rm -rf solana_program_library
rm -rf metaplex_program_library

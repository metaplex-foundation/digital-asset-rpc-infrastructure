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

pushd metaplex_program_library/candy-machine/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/mpl_candy_machine.so $CWD/programs/cndy3Z4yapfJBmL3ShUp5exZKqR3z33thTzeNMm2gRZ.so
popd

pushd metaplex_program_library/candy-machine-core/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/mpl_candy_machine_core.so $CWD/programs/CndyV3LdqHUfDLmE5naZjVN8rBZz4tqhdefbAnjHG3JR.so
popd

mkdir mpl_candy_guard || true
curl -LkSs https://api.github.com/repos/metaplex-foundation/mpl-candy-guard/tarball | tar -xz --strip-components=1 -C ./mpl_candy_guard

pushd mpl_candy_guard/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/mpl_candy_guard.so $CWD/programs/Guard1JwRhJkVH6XZhzoYxeBVQe872VH6QggF4BWmS9g.so
popd

mkdir solana_program_library || true
curl -LkSs https://api.github.com/repos/solana-labs/solana-program-library/tarball | tar -xz --strip-components=1 -C ./solana_program_library
tar -zxf -C /solana_program_library solana-program-library.tar.gz
pushd solana_program_library/account-compression/programs/account-compression
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_account_compression.so $CWD/programs/cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK.so
popd

pushd solana_program_library/account-compression/programs/noop
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_noop.so $CWD/programs/noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV.so
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
rm -rf mpl_candy_guard


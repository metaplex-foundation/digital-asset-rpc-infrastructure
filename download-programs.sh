#!/usr/bin/env bash

curl -LkSs https://api.github.com/repos/metaplex-foundation/metaplex-program-library/tarball | tar xz --strip=1
pushd metaplex-program-library/bubblegum/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/mpl_bubblegum.so ../../../programs/BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY.so
popd

curl -LkSs https://api.github.com/repos/solana-labs/solana-program-library/tarball -o solana-program-library.tar.gz
tar -zxf -C /solana-program-library solana-program-library.tar.gz
pushd solana-program-library/account-compression/programs/account-compression
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/spl_compression.so ../../../programs/GRoLLzvxpxxu2PGNJMMeZPyMxjAUH9pKqxGXV9DGiceU.so
popd
pushd solana-program-library/account-compression/programs/wrapper
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/wrapper.so ../../../programs/WRAPYChf58WFCnyjXKJHtrPgzKXgHp6MD9aVDqJBbGh.so
popd

pushd solana-program-library/associated-token-account/program
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/wrapper.so ../../../programs/ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL.so
popd

pushd solana-program-library/token/program-2022
  cargo build-bpf --bpf-out-dir ./here
  mv ./here/wrapper.so ../../../programs/TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb.so
popd

rm -rf solana-program-library
rm -rf metaplex-program-library

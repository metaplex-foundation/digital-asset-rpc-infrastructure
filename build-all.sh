#!/bin/bash
set -e

pushd nft_ingester
cargo build
popd

pushd digital_asset_types
cargo build
popd

pushd das_api
cargo build
popd

pushd migration
cargo build
popd

pushd metaplex-rpc-proxy
cargo build
popd
#!/bin/bash
set -e

pushd nft_ingester
cargo set-version $1
popd

pushd digital_asset_types
cargo set-version $1
popd

pushd das_api
cargo set-version $1
popd

pushd migration
cargo set-version $1
popd

pushd metaplex-rpc-proxy
cargo set-version $1
popd
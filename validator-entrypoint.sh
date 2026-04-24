#!/usr/bin/env bash
set -e

apt-get update -qq && apt-get install -y -qq wget > /dev/null 2>&1
wget -q http://archive.ubuntu.com/ubuntu/pool/main/o/openssl/libssl1.1_1.1.1f-1ubuntu2_amd64.deb
dpkg -i libssl1.1_1.1.1f-1ubuntu2_amd64.deb > /dev/null 2>&1
rm -f libssl1.1_1.1.1f-1ubuntu2_amd64.deb

exec ./runs.sh

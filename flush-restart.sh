#!/usr/bin/env bash
source ~/.bashrc
docker compose down -v

sudo rm -r db-data  && echo "db-data Flushed" || echo "db-data is empty"
sudo rm -r ledger  && echo "ledger Flushed" || echo "ledger is empty"

docker compose up db redis -d
sleep 30s

docker compose up solana migrator -d
sleep 60s

docker compose up ingester api proxy -d

cd ../hive-control

solana airdrop 10 -k keys/admin.json
solana airdrop 10 -k keys/driver.json
solana airdrop 10 -k keys/user.json
solana airdrop 10 -k key.json
yarn ts-node scripts/createGlobal.ts
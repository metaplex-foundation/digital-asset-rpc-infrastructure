## Dev process

### geyser gRPC source

Use [Triton One](https://triton.one/) provided endpoint or run own node with geyser plugin: https://github.com/rpcpool/yellowstone-grpc

### Redis server

```
redis-server
```

### PostgreSQL server

Run:

```
docker run -it --rm -e POSTGRES_PASSWORD=solana -e POSTGRES_USER=solana -e POSTGRES_DB=solana -p 5432:5432 postgres
```

Schema:

> Also note: The migration `m20230224_093722_performance_improvements` needs to be commented out of the migration lib.rs in order for the Sea ORM `Relations` to generate correctly.

```
DATABASE_URL=postgres://solana:solana@localhost/solana INIT_FILE_PATH=init.sql cargo run -p migration --bin migration -- up
```

psql:

```
PGPASSWORD=solana psql -h localhost -U solana -d solana
```

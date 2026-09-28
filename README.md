# sfn-local

A local stand-in for the AWS Step Functions API (`awsJson1.0`).

> **Status: skeleton.** The server accepts requests and routes them by `X-Amz-Target`, but no operation is implemented yet.
> Every known operation returns a `NotImplemented` error. See [docs/api.md](docs/api.md).

## Quick start

```bash
cargo run
# sfn-local listening on 0.0.0.0:8083

aws stepfunctions list-state-machines --endpoint-url http://localhost:8083
```

Or with Docker:

```bash
docker build -t sfn-local .
docker run --rm -p 8083:8083 sfn-local
```

## Documentation

- [docs/api.md](docs/api.md) — supported operations
- [docs/configuration.md](docs/configuration.md) — environment variables

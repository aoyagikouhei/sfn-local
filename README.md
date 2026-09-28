# sfn-local

A local stand-in for the AWS Step Functions API (`awsJson1.0`). It runs your ASL state machines in memory and
invokes Lambda functions through any Lambda Invoke API compatible endpoint (for example `cargo lambda watch` or LocalStack).

> **Status: early.** Standard workflows with Task (Lambda), Pass, Succeed and Fail states run end to end.
> See [docs/asl.md](docs/asl.md) for the supported subset and [docs/caveats.md](docs/caveats.md) for known differences.

## Quick start

Put your definitions in a directory as `<state machine name>.asl.json`:

```json
{
  "StartAt": "Invoke",
  "TimeoutSeconds": 300,
  "States": {
    "Invoke": {
      "Type": "Task",
      "Resource": "arn:aws:states:::lambda:invoke",
      "Parameters": { "FunctionName": "my-function", "Payload.$": "$" },
      "OutputPath": "$.Payload",
      "End": true
    }
  }
}
```

```bash
SFN_LOCAL_STATE_MACHINES_DIR=./state-machines \
SFN_LOCAL_LAMBDA_ENDPOINT=http://localhost:9000 \
cargo run
# sfn-local listening on 0.0.0.0:8083 (region: us-east-1, account: 123456789012, lambda endpoint: http://localhost:9000)
# state machine loaded: arn:aws:states:us-east-1:123456789012:stateMachine:example

aws stepfunctions start-execution --endpoint-url http://localhost:8083 \
  --state-machine-arn arn:aws:states:us-east-1:123456789012:stateMachine:example --input '{"n": 1}'
```

State machines can also be created at runtime with `CreateStateMachine`.

With Docker Compose:

```yaml
services:
  sfn:
    build: https://github.com/aoyagikouhei/sfn-local.git
    environment:
      SFN_LOCAL_STATE_MACHINES_DIR: /state-machines
      SFN_LOCAL_LAMBDA_ENDPOINT: http://lambda:9000
    volumes:
      - ./state-machines:/state-machines:ro
    ports:
      - "8083:8083"
```

## Documentation

- [docs/api.md](docs/api.md) — supported operations
- [docs/asl.md](docs/asl.md) — supported ASL subset and how Lambda is invoked
- [docs/configuration.md](docs/configuration.md) — environment variables
- [docs/caveats.md](docs/caveats.md) — known differences from Step Functions

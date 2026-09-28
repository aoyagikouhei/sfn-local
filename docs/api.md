# API

sfn-local speaks the Step Functions `awsJson1.0` protocol: every request is a `POST /` whose operation is named by the
`X-Amz-Target: AWSStepFunctions.<Operation>` header. SigV4 signatures are not verified.

## Operations

| Operation | Status |
|---|---|
| `CreateStateMachine` | not implemented |
| `DescribeStateMachine` | not implemented |
| `UpdateStateMachine` | not implemented |
| `DeleteStateMachine` | not implemented |
| `ListStateMachines` | not implemented |
| `StartExecution` | not implemented |
| `StartSyncExecution` | not implemented |
| `DescribeExecution` | not implemented |
| `StopExecution` | not implemented |
| `ListExecutions` | not implemented |
| `GetExecutionHistory` | not implemented |

A not-implemented operation returns HTTP 400 with:

```json
{"__type": "NotImplemented", "message": "sfn-local: <Operation> is not implemented yet"}
```

`NotImplemented` is specific to sfn-local; the real Step Functions never returns it. It is a 4xx so that SDKs do not retry.

A missing header, a missing `AWSStepFunctions.` prefix, or an unknown operation name returns HTTP 400 with
`{"__type": "UnknownOperationException"}` (the real service's shape is not measured yet).

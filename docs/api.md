# API

sfn-local speaks the Step Functions `awsJson1.0` protocol: every request is a `POST /` whose operation is named by the
`X-Amz-Target: AWSStepFunctions.<Operation>` header. SigV4 signatures are not verified; any credentials work.

## Operations

| Operation | Status |
|---|---|
| `CreateStateMachine` | supported (`name`, `definition`, `roleArn`, `type`) |
| `DescribeStateMachine` | supported |
| `ListStateMachines` | supported (no paging; everything in name order) |
| `StartExecution` | supported |
| `DescribeExecution` | supported |
| `UpdateStateMachine` | not implemented |
| `DeleteStateMachine` | not implemented |
| `StartSyncExecution` | not implemented |
| `StopExecution` | not implemented |
| `ListExecutions` | not implemented |
| `GetExecutionHistory` | not implemented |

State machines and executions live in memory and are lost on restart.

### CreateStateMachine

- Creating the same name again with the same definition and role returns the existing state machine; a different
  definition or role fails with `StateMachineAlreadyExists`.
- A definition sfn-local cannot run fails with `InvalidDefinition` and a message starting with
  `UNSUPPORTED_BY_SFN_LOCAL:` (see [asl.md](asl.md)).
- The ARN is `arn:aws:states:<SFN_LOCAL_REGION>:<SFN_LOCAL_ACCOUNT_ID>:stateMachine:<name>`.

### StartExecution

Validation follows the order measured on the real service: ARN shape (`InvalidArn`), model constraints
(`ValidationException`: name up to 80 characters, input up to 262144 bytes), state machine existence
(`StateMachineDoesNotExist`), name characters (`InvalidName`) and input JSON (`InvalidExecutionInput`), and finally an
existing execution with the same name. Starting again with the same name and byte-identical input while the first
execution is `RUNNING` returns the first execution; otherwise it fails with `ExecutionAlreadyExists`.

A missing `name` gets a UUID; a missing `input` becomes `{}`.

The execution ARN replaces `:stateMachine:` with `:execution:` and appends the name.

### DescribeExecution

Keys are omitted rather than set to `null`: `stopDate` only after the execution ends, `output` only for `SUCCEEDED`,
`error` and `cause` only for `FAILED`. `TIMED_OUT` has neither `error` nor `cause`.

## Errors

Errors are HTTP 400 with `{"__type": "<namespace>#<name>", "message": "..."}`. Service errors use the
`com.amazonaws.swf.service.v2.model` namespace and `ValidationException` uses `com.amazon.coral.validate`, as the real
service does. A body that is not a JSON object, or a field of the wrong type, returns `{"__type": "SerializationException"}`.

A not-implemented operation returns `{"__type": "NotImplemented", "message": "sfn-local: <Operation> is not implemented yet"}`.
`NotImplemented` is specific to sfn-local; it is a 4xx so that SDKs do not retry.

A missing header, a missing `AWSStepFunctions.` prefix, or an unknown operation name returns
`{"__type": "UnknownOperationException"}`.

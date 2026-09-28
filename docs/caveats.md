# Caveats

Known differences from the real Step Functions.

- Only the operations and ASL features listed in [api.md](api.md) and [asl.md](asl.md) exist.
- State machines and executions are kept in memory only.
- `STANDARD` and `EXPRESS` state machines behave the same way (as `STANDARD`).
- Responses omit fields sfn-local does not track: `DescribeExecution` has no `inputDetails`, `outputDetails` or
  `redriveCount`; `DescribeStateMachine` has no `loggingConfiguration` or `tracingConfiguration`.
- The `lambda:invoke` result has no `SdkHttpMetadata` or `SdkResponseMetadata`.
- `ListStateMachines` does not page and returns state machines in name order (the real order is not measured).
- Messages inside `InvalidExecutionInput` and `InvalidDefinition` come from sfn-local's own parser and differ from the real ones.
- Not measured on the real service, so sfn-local follows the documentation or the same protocol elsewhere:
  `CreateStateMachine` validation order and messages, `SerializationException` for unreadable bodies,
  `UnknownOperationException`, the Lambda error names for non-function failures (`Lambda.ServiceException`,
  `Lambda.SdkClientException`), and not applying `OutputPath` when a `Catch` transitions.

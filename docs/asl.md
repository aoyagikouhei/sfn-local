# ASL support

sfn-local interprets Amazon States Language definitions itself. Anything it cannot run is rejected when the definition
is loaded (at startup or by `CreateStateMachine`) with a message starting with `UNSUPPORTED_BY_SFN_LOCAL:`, instead of
failing halfway through an execution.

## Supported

| Feature | Notes |
|---|---|
| States | `Task`, `Pass`, `Succeed`, `Fail` |
| Paths | `InputPath`, `OutputPath`, `ResultPath`, including `null` |
| Templates | `Parameters`, `ResultSelector` with `.$` fields |
| JSONPath | reference paths only: `$`, `$.a.b`, `$['a b']`, `$.a[0]`, and the context object `$$` |
| Context object | `$$.Execution.{Id,Input,Name,RoleArn,StartTime}`, `$$.StateMachine.{Id,Name}`, `$$.State.{Name,EnteredTime,RetryCount}` |
| Errors | `Retry` (`IntervalSeconds`, `MaxAttempts`, `BackoffRate`, `MaxDelaySeconds`), `Catch` (`ResultPath`), `States.ALL`, `States.TaskFailed` |
| Timeouts | top-level `TimeoutSeconds` (the execution becomes `TIMED_OUT`), Task `TimeoutSeconds` (`States.Timeout`) |

A path that does not resolve at runtime fails the execution with `States.Runtime`, which `Retry` and `Catch` do not match.

## Not supported yet

`Choice`, `Wait`, `Parallel` and `Map` states; intrinsic functions (`States.Format` and friends); JSONata;
`Assign`; `ErrorPath` / `CausePath` in `Fail`; JSONPath wildcards, filters and `..`; `HeartbeatSeconds` is ignored;
`JitterStrategy` is ignored.

## Task resources and Lambda

| `Resource` | Result |
|---|---|
| `arn:aws:states:::lambda:invoke` | `{"ExecutedVersion", "Payload", "StatusCode"}`. `Parameters` must contain `FunctionName`; `Payload` is sent as the request body and `InvocationType` as `X-Amz-Invocation-Type`. |
| `arn:aws:lambda:<region>:<account>:function:<name>[:<qualifier>]` | the function's return value; the effective input is sent as the request body. |

Other resources (other service integrations, `.sync`, `.waitForTaskToken`, activities) are rejected.

Functions are invoked with `POST <SFN_LOCAL_LAMBDA_ENDPOINT>/2015-03-31/functions/<name>/invocations`, without signing.
The function name is taken from a plain name, a full ARN or a partial ARN; a version or alias becomes `?Qualifier=`.
Use the name your local Lambda runtime registers the function under.

| Lambda response | Task result |
|---|---|
| 2xx without `X-Amz-Function-Error` | success. The body is not inspected, so `{"error": ...}` is still a success. |
| 2xx with `X-Amz-Function-Error` | error `errorType` from the body, cause = the body |
| non-2xx whose body has `errorType` | same as above (`cargo lambda watch` reports function errors as HTTP 500) |
| other non-2xx | `Lambda.<x-amzn-ErrorType>` if the header is present, otherwise `Lambda.ServiceException` |
| connection failure, or no endpoint configured | `Lambda.SdkClientException` |

sfn-local does not add retries of its own: if your definition has the `Retry` that the CDK `LambdaInvoke` construct adds by
default, a Lambda that is not up yet is retried with that backoff.

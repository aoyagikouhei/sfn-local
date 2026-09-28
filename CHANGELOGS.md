# Changelog

## [Unreleased]

- Add the server skeleton: `POST /` dispatch by `X-Amz-Target` ([docs/api.md](docs/api.md)).
- Add `CreateStateMachine`, `DescribeStateMachine`, `ListStateMachines`, `StartExecution` and `DescribeExecution` ([docs/api.md](docs/api.md)).
- Run ASL definitions with Task (Lambda), Pass, Succeed and Fail states, paths, Retry, Catch and timeouts ([docs/asl.md](docs/asl.md)).
- Load state machines from `SFN_LOCAL_STATE_MACHINES_DIR` at startup ([docs/configuration.md](docs/configuration.md)).

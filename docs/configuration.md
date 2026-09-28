# Configuration

All settings come from environment variables. An empty value stops startup.

| Variable | Default | Description |
|---|---|---|
| `SFN_LOCAL_BIND` | `0.0.0.0:8083` | Address to listen on. |
| `SFN_LOCAL_REGION` | `us-east-1` | Region in state machine ARNs. |
| `SFN_LOCAL_ACCOUNT_ID` | `123456789012` | Account ID in state machine ARNs (digits only). |
| `SFN_LOCAL_STATE_MACHINES_DIR` | none | Directory of `<name>.asl.json` files registered as state machines at startup. Other files are ignored. |
| `SFN_LOCAL_LAMBDA_ENDPOINT` | none | Lambda Invoke API endpoint (`http://` only). Without it, Lambda tasks fail with `Lambda.SdkClientException`. |

State machines loaded from the directory are `STANDARD` with the role `arn:aws:iam::<account>:role/sfn-local`.
If any file cannot be loaded (invalid or unsupported definition, name that is not a valid state machine name), sfn-local
prints the reason and exits with status 1.

To use ARNs that match another environment, set the region and account ID to the values in those ARNs.

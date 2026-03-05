# Feature Differentiation Matrix

Use this to avoid choosing the wrong primitive.

| Capability | query | mutation | job | cron | workflow | webhook | mcp_tool |
|---|---|---|---|---|---|---|---|
| Read DB | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Write DB | No (should not) | Yes | Yes | Yes | Yes | Yes | Yes |
| User request/response | Yes | Yes | Indirect | No | Indirect | Yes | Yes |
| Built-in scheduling | No | No | No | Yes | Timed sleep | External trigger | No |
| Durable retries | No | No | Yes | Runtime-level | Step-level | Via dispatched jobs | No |
| Long-running orchestration | No | Limited | Limited | Limited | Yes | No | No |
| Signature/idempotency ingress | No | No | No | No | No | Yes | No |
| AI-tool callable interface | No | No | No | No | No | No | Yes |

## Selection Rules

- Need immediate user-facing read: `query`
- Need immediate write: `mutation`
- Need async processing: `job`
- Need recurring time schedule: `cron`
- Need durable multi-step saga/sleep/event wait: `workflow`
- Need provider callback endpoint: `webhook`
- Need explicit agent tooling: `mcp_tool`

## Common Wrong Choices

- Using workflow for a single async task -> use mutation + job.
- Doing heavy sync work inside webhook -> dispatch job and return quickly.
- Putting writes in query -> use mutation.
- Using cron for one-off delayed sequence -> workflow with sleep.

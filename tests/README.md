# Integration Tests

The integration tests in this directory require external services and are marked with `#[ignore]` so they do not run in CI by default.

## Prerequisites

1. A running target registry, for example:

   ```bash
   docker run -d -p 5000:5000 --name registry registry:2
   ```

2. Network access to the source registry (default: `hubproxy.example.com`).

## Running

```bash
cargo test --test sync_integration_test -- --ignored
```

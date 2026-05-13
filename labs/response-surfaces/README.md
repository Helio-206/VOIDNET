# Response Surfaces Lab

This lab validates runtime-safe response rendering.

## Goal

Confirm that JSON, plain text, and HTML responses are converted into terminal-safe runtime response surfaces.

## Scenarios

1. `text/plain`

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://example.gateway.void/plain
```

Expected:
- `gateway.response_status` reports `text/plain`.
- `gateway.response_preview` renders the response body directly.

2. `application/json`

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://example.gateway.void/json
```

Expected:
- `gateway.response_status` reports `application/json`.
- `gateway.response_preview` shows pretty-printed JSON.

3. `text/html`

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://example.gateway.void/html
```

Expected:
- `gateway.response_status` reports `text/html`.
- `gateway.response_preview` uses the HTML fallback renderer instead of full HTML execution.

4. Invalid response

Expected:
- The bridge records a failure in diagnostics.
- `gateway_bridge_failures` increases.

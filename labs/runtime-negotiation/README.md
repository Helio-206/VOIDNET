# Runtime Negotiation Lab

Purpose: validate capability negotiation and persisted permission decisions.

## Publish A Sensitive Surface

Use a local DNS cache containing a route that requests sensitive capabilities such as `storage` or `filesystem`.

The runtime test suite already covers this path automatically, but the operational flow is:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-negotiation runtime permissions
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-negotiation runtime permissions grant vault 12D3Koo... storage
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-negotiation runtime permissions deny vault 12D3Koo... filesystem
```

Expected result:

- granted capabilities persist in the runtime shell state
- denied capabilities remain visible in the permission list
- unsupported sensitive routes fail with a recorded failed mount until an explicit grant exists
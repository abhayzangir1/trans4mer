# Plugin Development

Trans4mers plugins are processes, not libraries: your code runs out-of-process and talks JSON-RPC over stdio. This is the security model — a plugin crash or hang cannot take down the engine, and the sandbox enforces timeout, output size, and concurrency limits.

## Contract

1. Put a `manifest.json` in a directory under the plugin root:
   ```json
   {
     "name": "example-plugin",
     "version": "1.0.0",
     "description": "...",
     "command": ["python3", "server.py"],
     "tools": [
       {
         "name": "echo",
         "description": "Echo text back",
         "input_schema": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] }
       }
     ]
   }
   ```
2. Implement the server: read JSON requests (one per line) from stdin, write JSON responses to stdout.
   - Request: `{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hi"}}}`
   - Response: `{"jsonrpc":"2.0","id":1,"result":{"content":"echo: hi"}}`
3. Flush stdout after every response.

See `plugins/example-plugin/server.py` for a complete ~40-line reference implementation.

## Sandbox limits (defaults)

| Limit | Value |
|---|---|
| Execution timeout | 30 s |
| Output size | 10 MB |
| Concurrent calls per plugin | 3 |

Limits are enforced by `PluginManager` in `trans4mers-providers/src/plugin/mod.rs`.

## Tool safety

Plugin tools are registered with `EffectClass::Unknown` and `RiskLevel::Medium` — unknown safety is treated as non-idempotent: failures never auto-retry, and the policy engine will `Ask` before first use. Grant the `plugin:<tool>` capability to allow-list specific tools per agent.

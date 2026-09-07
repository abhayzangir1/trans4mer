#!/usr/bin/env python3
"""Example Trans4mers plugin: stdio JSON-RPC server.

Protocol (deliberately minimal and stable):
- Request:  {"jsonrpc":"2.0","id":N,"method":"tools/call","params":{"name":...,"arguments":{...}}}
- Response: {"jsonrpc":"2.0","id":N,"result":{"content":"..."}}
One JSON object per line on stdout; reads from stdin.
"""
import json
import sys


def handle_call(params):
    name = params.get("name", "")
    arguments = params.get("arguments", {})
    if name == "echo":
        text = arguments.get("text", "")
        return f"echo: {text}"
    return f"unknown tool: {name}"


def main() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue
        request_id = request.get("id")
        if request.get("method") == "tools/call":
            result = handle_call(request.get("params", {}))
        else:
            result = "method not supported"
        response = {"jsonrpc": "2.0", "id": request_id, "result": {"content": result}}
        sys.stdout.write(json.dumps(response) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()

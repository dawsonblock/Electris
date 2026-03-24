# Tool Sandbox Notes

The corrected build preserves the intended sandbox boundary but is not compile-verified in this environment.

## Intended execution chain

```text
policy → validation → sandbox runner → output cap → audit event
```

## Minimum restrictions

- no network by default
- read-only filesystem except mounted workspace roots
- memory cap
- CPU cap
- PID cap
- timeout
- output truncation

## Audit points

Run these after compiling:

```bash
rg -n 'Command::new' src crates
rg -n 'process_message\(' src crates
```

Interpretation:
- host command execution should exist only in the sandbox layer
- request execution should exist only in the worker path

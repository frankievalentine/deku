---
title: Logs
description: Build and runtime logs for every deployment, stored and searchable.
---

Deku stores build output and runtime output for every deployment. Because lines are stored rather
than read from live containers, a deployment's logs stay readable after its containers are retired,
which is when you usually want them.

## Sources

Every line is tagged with:

| Field | Meaning |
| --- | --- |
| `source` | `build` (image build and rollout output) or `runtime` (container stdout/stderr) |
| `stream` | `stdout` or `stderr` |
| `level` | Best-effort level inferred from the line: `INFO`, `WARNING`, `ERROR`, `DEBUG`, `CRITICAL` |
| `deployment` | The deployment that produced the line |
| `environment` | The environment that deployment belongs to |

Level is inferred from the text (a `[ERROR]` tag, an `ERROR:` prefix, `level=error`, and so on),
because build and app output is plain text with no structured level. It is only used for filtering
and colouring; use `stream` when you need to isolate stderr exactly.

## Reading logs

```bash
deku logs <app>                      # most recent lines, build and runtime
deku logs <app> -n 500               # more history
deku logs <app> --follow             # live
deku logs <app> --follow --timeout 30
```

Filtering:

```bash
deku logs <app> --source build       # only build output
deku logs <app> --stream stderr      # only stderr
deku logs <app> --level ERROR        # only lines that look like errors
deku logs <app> --deployment <id>    # one deployment, including retired ones
deku logs <app> --environment staging
deku logs <app> --search "connection refused"
```

`--search` is a case-insensitive full-text search over stored lines. Terms are matched
independently, so `--search "connection refused"` finds lines containing both words. Special
characters in a search term are treated as text, so pasting a timestamp or a URL will not error.

The same filters are available over HTTP:

```
GET /api/apps/{name}/logs?n=200&search=timeout&source=runtime&level=ERROR
GET /api/apps/{name}/logs/stream          # SSE, same filters, live
```

`GET /api/apps/{name}/logs` returns structured `lines` alongside the plain `logs` string array, so
older clients keep working.

## Live streaming

`--follow` streams stored log lines as they arrive, so you see runtime output and build output in
one place rather than only lifecycle events. Each line is delivered as soon as it is captured, over
the same SSE mechanism the event feed uses.

Runtime capture starts the moment a container is created, so a container's output is recorded from
its first line. After a daemon restart, Deku reattaches to the containers that are still running and
continues from that point rather than re-ingesting history.

## Retention

Logs are bounded per app so a chatty app cannot fill the disk:

```toml
[logs]
retain_lines = 20000   # newest lines kept per app; older lines are pruned
```

Pruning runs on a timer (every five minutes) and keeps the newest lines per app. Pruned lines leave
the search index too. Nothing else in Deku is affected: the app, its deployments, and its other
state are untouched.

## Design notes

- Lines live in `log_lines` in the same SQLite database as everything else, so they are covered by
  the same file permissions and encryption-at-rest story as config vars.
- Search uses **FTS5**, which the SQLite Deku bundles is compiled with. There is no external log
  service, no second port, and no additional runtime dependency.
- Log writes are best-effort: a failure to store a line never fails a deploy or a running app.

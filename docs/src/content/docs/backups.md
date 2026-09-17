---
title: Backups
description: Back up managed datastores on demand, or on a schedule with automatic retention.
---

Deku backs up managed Postgres, MySQL, MariaDB, Redis, and MongoDB services to the configured object store. Run a backup when you need one, or attach a schedule that backs up and prunes on its own.

Both paths need an object store. Verify it before you rely on backups:

```bash
deku objectstore test
```

## On-demand backups

```bash
deku postgres backup my-db
deku postgres backups my-db
deku postgres restore my-db <backup-id>

deku redis backup my-cache
deku redis backups my-cache
deku redis restore my-cache <backup-id>

deku mysql backup my-mysql
deku mysql backups my-mysql
deku mysql restore my-mysql <backup-id>
```

`backup` writes a dump to the object store and records it against the service. `backups` lists what is stored. `restore` loads a backup back into the service and overwrites its current data, so check the id with `backups` first.

## Scheduled backups

```bash
deku backup schedule my-db --interval-hours 24 --keep 7
deku backup status my-db
deku backup schedules
deku backup unschedule my-db
```

`--interval-hours` sets how often the backup runs and `--keep` sets how many to retain. Both have defaults: `24` hours and `7` backups. After each run, Deku deletes the oldest backups until only `--keep` remain, removing them from the object store and from the backup list.

The scheduler checks every minute, so a run that comes due while the daemon is stopped happens shortly after it starts again.

`deku backup schedules` lists every service with a schedule, including the last run status and the next run time.

## Encryption at rest

One key protects two kinds of sensitive value the daemon writes: service backups uploaded to an
object store, and config var values stored in the state database. Both are sealed with AES-256-GCM
before they are written, so the object store only ever holds ciphertext and a copy of the database
does not leak your API keys.

```toml
[encryption]
# 64 hex characters or base64-encoded 32 bytes
key_file = "/etc/deku/encryption.key"
```

```bash
openssl rand -hex 32 > /etc/deku/encryption.key
chmod 600 /etc/deku/encryption.key
```

`key` accepts the value inline instead of `key_file`, and `DEKU_ENCRYPTION_KEY` overrides both.
Prefer `key_file` or the environment variable so the key stays out of the config file.

### Backups

Each dump is sealed before upload and the envelope is self-describing, so:

- Backups written before encryption was enabled still restore.
- An encrypted backup cannot be restored without the key; restore fails with a clear error rather
  than writing garbage.
- The wrong key fails closed, because GCM authenticates the payload.

The checksum recorded in the database covers the stored (sealed) bytes. `deku <engine> backups
<service>` shows the `ENCRYPTION` column, and the dashboard shows the same per backup.

### Config vars

Config var values are encrypted on write and decrypted on read, and the decryption is transparent:

- `deku config list` prints plaintext, matching what gets injected into your container.
- Values written before encryption was enabled keep working.
- Values are re-read on every deploy, so rotating the key is: stop the daemon, re-encrypt with the
  new key, start it. Backups written under the old key are only readable with that key.

If the key is missing or wrong, Deku does not silently degrade:

- `deku config list` marks the value `unreadable` and shows why, instead of printing an empty value.
- A deploy fails with `config var '<key>' could not be decrypted`, so a broken secret never reaches
  a running app.
- `deku doctor` reports `encryption_at_rest`: `ok` with the count of encrypted values, `warn` when no
  key is set and nothing is encrypted, and `fail` when ciphertext exists that cannot be read.

With no key configured, values and backups are stored in the clear and `deku doctor` warns about it.

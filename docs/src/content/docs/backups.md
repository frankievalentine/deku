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

## Encrypting backups

Backups are uploaded in plaintext unless a key is configured. With a key set, each dump is sealed
with AES-256-GCM before it leaves the host, so the object store only ever holds ciphertext. The
stored object is a self-describing envelope (`magic || nonce || ciphertext || tag`), and the
checksum recorded in the database covers the stored bytes.

Set the key in the daemon config:

```toml
[backup_encryption]
# 64 hex characters or base64-encoded 32 bytes
key_file = "/etc/deku/backup.key"
```

```bash
openssl rand -hex 32 > /etc/deku/backup.key
chmod 600 /etc/deku/backup.key
```

`key` accepts the value inline instead of `key_file`, and `DEKU_BACKUP_KEY` overrides both. Prefer
`key_file` or the environment variable so the key stays out of the config file.

Restores detect the envelope automatically, so:

- Backups written before encryption was enabled still restore.
- An encrypted backup cannot be restored without the key. Restore fails with a clear error rather
  than writing garbage.
- The wrong key fails closed; GCM authenticates the payload.

`deku postgres backups <service>` shows the `ENCRYPTION` column, and the dashboard shows the same
per backup.

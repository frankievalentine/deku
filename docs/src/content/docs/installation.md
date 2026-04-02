---
title: Installation
description: How to install Deku on your server
---

# Installation

## Requirements

- Linux server (Ubuntu 22.04+ or Debian 12+)
- Docker Engine 24+
- 512 MB RAM minimum

## One-line Install

```bash
curl -sSL https://get.deku.sh | bash
```

This will:

1. Install Angie (reverse proxy)
2. Download the latest `dekud` and `deku` binaries
3. Create the systemd service
4. Run `dekud setup`

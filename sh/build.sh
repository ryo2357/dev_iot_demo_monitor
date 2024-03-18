#!/usr/bin/bash
# ルートフォルダから実行されつことを想定

sudo docker compose -f docker-compose.production.yml build --no-cache
exit 0
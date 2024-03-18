#!/usr/bin/bash
# ルートフォルダから実行されつことを想定
docker compose -f docker-compose.production.yml stop
exit 0
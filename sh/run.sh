#!/usr/bin/bash
# ルートフォルダから実行されつことを想定
docker compose -f docker-compose.production.yml up -d
exit 0
# iot_demo_monitor

IoT gateway ＋ データベース ＋ ダッシュボードの実装

ローカルネットワーク内で完結する構成

## Usege

### 設定

``$ docker compose up -d``

influxDBとGrafanaの設定

``$ docker compose down``


### 稼働

``$ bash prod.sh``

## Note

IoT gateway:Rustにより実装。キーエンスPLCとの通信を想定

データベース:influxDB

ダッシュボード:Grafana

ダッシュボードの設定はGitで管理をおこな
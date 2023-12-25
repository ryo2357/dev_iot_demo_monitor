use futures::stream;
use influxdb2::models::DataPoint;
use influxdb2::Client;
use log::{debug, error};
use tokio::sync::mpsc;

pub async fn send_data(mut rx: mpsc::Receiver<Vec<DataPoint>>) -> anyhow::Result<()> {
    // クライアント設定
    let host = std::env::var("INFLUXDB_HOST").unwrap();
    let org = std::env::var("INFLUXDB_ORG").unwrap();
    let token = std::env::var("INFLUXDB_TOKEN").unwrap();
    let bucket = std::env::var("INFLUXDB_BUCKET").unwrap();

    let client = Client::new(host, org, token);

    // データ送信処理

    while let Some(points) = rx.recv().await {
        debug!("receive {:?} data", points.len());

        let result = client.write(&bucket, stream::iter(points)).await;

        match result {
            Ok(()) => {}
            Err(r) => {
                error!("{:?}", r)
            }
        }
    }

    Ok(())
}

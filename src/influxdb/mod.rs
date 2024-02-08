use futures::stream;
use influxdb2::models::DataPoint;
use influxdb2::Client;
use log::{debug, error};
use tokio::sync::mpsc;

use tokio::task::JoinHandle;

pub struct InfluxDB {
    host: String,
    org: String,
    token: String,
    bucket: String,
    send_thread: Option<JoinHandle<()>>,
}

impl InfluxDB {
    pub fn create_from_env() -> anyhow::Result<Self> {
        let host = std::env::var("INFLUXDB_HOST")?;
        let org = std::env::var("INFLUXDB_ORG")?;
        let token = std::env::var("INFLUXDB_TOKEN")?;
        let bucket = std::env::var("INFLUXDB_BUCKET")?;

        Ok(Self {
            host,
            org,
            token,
            bucket,
            send_thread: None,
        })
    }
    pub async fn start_send_data(
        &mut self,
        mut rx: mpsc::Receiver<Vec<DataPoint>>,
    ) -> anyhow::Result<()> {
        if self.send_thread.is_some() {
            anyhow::bail!("already making data in InfluxDB::start_making_data")
        }

        debug!("start_send_data");
        let client = Client::new(self.host.clone(), self.org.clone(), self.token.clone());
        let bucket = self.bucket.clone();
        let thread = tokio::spawn(async move {
            while let Some(points) = rx.recv().await {
                let receive_num = points.len();
                let result = client.write(&bucket, stream::iter(points)).await;

                match result {
                    Ok(()) => {
                        debug!("{:?}データを送信完了", receive_num)
                    }
                    Err(r) => {
                        error!("{:?}", r)
                    }
                }
            }
        });

        self.send_thread = Some(thread);

        Ok(())
    }
    pub async fn wait_thread_finished(&mut self) -> anyhow::Result<()> {
        if let Some(thread) = self.send_thread.take() {
            debug!("wait thread finished");
            thread.await?;
        } else {
            anyhow::bail!("not start send_thread in InfluxDB::wait_thread_finished")
        }

        debug!("confirmed thread finished");
        Ok(())
    }
}

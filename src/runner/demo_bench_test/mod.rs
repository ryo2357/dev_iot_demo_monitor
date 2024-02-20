use log::{info, warn};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::Duration;

use crate::collector::demo_cpb16::DemoCpb16Collector;
use crate::influxdb::InfluxDB;

pub struct Runner {
    collector: Arc<Mutex<DemoCpb16Collector>>,
    database: InfluxDB,
}

impl Runner {
    pub async fn create_from_env() -> anyhow::Result<Self> {
        let (data_sender, data_receiver) = mpsc::channel(32);
        let mut database = InfluxDB::create_from_env()?;
        // senderはドロップされないのでdatabaseの終了処理は不要
        database.start_send_data(data_receiver).await?;
        let collector = DemoCpb16Collector::create_from_env(data_sender).await?;

        let collector = Arc::new(Mutex::new(collector));
        let database = database;

        Ok(Self {
            collector,
            database,
        })
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn execute(&mut self) -> anyhow::Result<()> {
        let (disconnect_sender, mut disconnect_receiver) = mpsc::channel(32);
        {
            let sender = disconnect_sender.clone();
            let mut collector = self.collector.lock().unwrap();
            collector.start_data_collection(sender).await?;
        }
        info!("start data collect");

        while let Some(()) = disconnect_receiver.recv().await {
            warn!("The connection with the PLC has been lost");
            loop {
                info!("Reconnect after 20 seconds");
                wait(20).await;
                let sender = disconnect_sender.clone();
                {
                    let mut collector = self.collector.lock().unwrap();
                    match collector.start_data_collection(sender).await {
                        Ok(()) => {
                            info!("Reconnection Successful");
                            break;
                        }
                        Err(r) => {
                            warn!("connection_failure:{:?}", r);
                        }
                    }
                }
            }
        }

        warn!("disconnect_sender was drop");

        Ok(())
    }
}

async fn wait(sec: u64) {
    tokio::time::sleep(Duration::from_secs(sec)).await;
}

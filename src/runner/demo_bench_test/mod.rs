use std::thread::JoinHandle;

use tokio::sync::mpsc;

use crate::collector::demo_machine::DemoMachineCollector;
use crate::influxdb::InfluxDB;

pub struct Runner {
    is_execute: bool,
    task: Option<Job>,
    thread: Option<JoinHandle<Task>>,
}
impl Runner {
    pub async fn create_from_env() -> anyhow::Result<Self> {
        let task = Job::create_from_env()?;
        Ok(Self {
            is_execute: false,
            task: Some(task),
            thread: None,
        })
    }

    pub async fn execute() -> anyhow::Result<()> {
        let (disconnect_sender, disconnect_receiver) = mpsc::channel::<()>(32);

        while let Some(points) = disconnect_receiver.recv().await {
            self.task.hey().await;
        }
        Ok(())
    }
}

struct Job {
    collector: DemoMachineCollector,
    database: InfluxDB,
}

impl Job {
    async fn create_from_env() -> anyhow::Result<Self> {
        let (data_sender, data_receiver) = mpsc::channel(32);
        let mut database = InfluxDB::create_from_env()?;
        // senderはドロップされないのでdatabaseの終了処理は不要
        database.start_send_data(data_receiver).await?;
        let collector = DemoMachineCollector::create_from_env(data_sender).await?;

        Ok(Self {
            collector,
            database,
        })
    }

    async fn hey() -> anyhow::Result<()> {
        Ok(())
    }
}

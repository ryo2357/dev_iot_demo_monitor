use influxdb2::models::DataPoint;

use tokio::sync::mpsc;

use super::config::DemoMachineConfig;
use super::data_manager::DemoMachineDataManager;
use super::interface::DemoMachineInterface;

pub struct DemoMachineCollecter {
    data_sender: mpsc::Sender<Vec<DataPoint>>,
    interface: DemoMachineInterface,
    manager: Option<DemoMachineDataManager>,
}

impl DemoMachineCollecter {
    pub async fn create_from_env(
        data_sender: mpsc::Sender<Vec<DataPoint>>,
    ) -> anyhow::Result<Self> {
        let config = DemoMachineConfig::create_from_env()?;
        let interface = DemoMachineInterface::create_from_config(config).await?;
        Ok(Self {
            data_sender,
            interface,
            manager: None,
        })
    }

    pub async fn start_data_collection(&mut self) -> anyhow::Result<()> {
        if self.interface.is_monitoring() {
            anyhow::bail!("start_data_collection can not execute: interface is monitoring")
        }
        let (point_sender, point_receiver) = mpsc::channel(32);
        let data_sender = self.data_sender.clone();
        let manager = DemoMachineDataManager::create(data_sender, point_receiver)?;
        self.interface.start_moniter(point_sender).await?;
        self.manager = Some(manager);
        Ok(())
    }

    pub async fn stop_data_collection(&mut self) -> anyhow::Result<()> {
        if !self.interface.is_monitoring() {
            anyhow::bail!("stop_data_collection can not execute: interface is not monitoring")
        }
        // ここでpoint_senderがドロップされる
        self.interface.stop_moniter().await?;
        // DemoMachineDataManager内のスレッドが終了されているはず
        let Some(manager) = self.manager.take() else {
            anyhow::bail!("想定しないないエラー")
        };
        manager.wait_thread_finished().await?;

        Ok(())
    }
}

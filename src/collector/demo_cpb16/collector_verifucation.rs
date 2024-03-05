use influxdb2::models::DataPoint;

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use super::config::DemoCpb16Config;
use super::data_manager::DemoCpb16DataManager;
use super::interface::DemoCpb16Interface;

pub struct DemoCpb16Collector {
    interface: Arc<Mutex<DemoCpb16Interface>>,
    manager: DemoCpb16DataManager,
    is_running: bool,
}

impl DemoCpb16Collector {
    pub async fn create_from_env(
        data_sender: mpsc::Sender<Vec<DataPoint>>,
    ) -> anyhow::Result<Self> {
        let config = DemoCpb16Config::create_from_env()?;
        let interface = DemoCpb16Interface::create_from_config(config).await?;
        let manager = DemoCpb16DataManager::create(data_sender)?;

        let interface = Arc::new(Mutex::new(interface));
        Ok(Self {
            interface,
            manager,
            is_running: false,
        })
    }
    #[allow(clippy::await_holding_lock)]
    pub async fn start_data_collection(
        &mut self,
        disconnect_sender: mpsc::Sender<()>,
    ) -> anyhow::Result<()> {
        if self.is_running {
            anyhow::bail!("start_data_collection can not execute: collector is running")
        }

        let (point_sender, point_receiver) = mpsc::channel(32);
        self.manager.create_thread(point_receiver).await?;

        {
            let mut interface = self.interface.lock().unwrap();
            interface
                .start_monitor(point_sender, disconnect_sender)
                .await?;
        }
        self.is_running = true;
        Ok(())
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn stop_data_collection(&mut self) -> anyhow::Result<()> {
        if !self.is_running {
            anyhow::bail!("stop_data_collection can not execute: collector is not running")
        }
        // ここでpoint_senderがドロップされる
        {
            let mut interface = self.interface.lock().unwrap();
            interface.stop_monitor().await?;
        }
        // DemoCpb16DataManager内のスレッドが終了されているはず
        self.manager.finish_thread().await?;

        Ok(())
    }

    // pub async fn start_data_collection(
    //     &mut self,
    //     disconnect_sender: mpsc::Sender<()>,
    // ) -> anyhow::Result<()> {
    //     if self.interface.is_monitoring() {
    //         anyhow::bail!("start_data_collection can not execute: interface is monitoring")
    //     }
    //     if self.manager.have_thread() {
    //         anyhow::bail!("start_data_collection can not execute: manager have thread")
    //     }
    //     let (point_sender, point_receiver) = mpsc::channel(32);
    //     self.manager.create_thread(point_receiver).await?;
    //     self.interface
    //         .start_monitor(point_sender, disconnect_sender)
    //         .await?;

    //     Ok(())
    // }

    // pub async fn stop_data_collection(&mut self) -> anyhow::Result<()> {
    //     if !self.interface.is_monitoring() {
    //         anyhow::bail!("stop_data_collection can not execute: interface is not monitoring")
    //     }
    //     if !self.manager.have_thread() {
    //         anyhow::bail!("stop_data_collection can not execute: manager do not have thread")
    //     }
    //     // ここでpoint_senderがドロップされる
    //     self.interface.stop_monitor().await?;
    //     // DemoCpb16DataManager内のスレッドが終了されているはず
    //     self.manager.finish_thread().await?;

    //     Ok(())
    // }
}
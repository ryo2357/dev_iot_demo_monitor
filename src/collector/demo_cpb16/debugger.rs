use log::debug;
use tokio::sync::mpsc;

use super::config::DemoCpb16Config;
use super::interface::DemoCpb16Interface;

pub struct DemoCpb16Debugger {
    interface: DemoCpb16Interface,
}

impl DemoCpb16Debugger {
    pub async fn create_from_env() -> anyhow::Result<Self> {
        let config = DemoCpb16Config::create_from_env()?;
        let interface = DemoCpb16Interface::create_from_config(config)?;
        Ok(Self { interface })
    }

    pub async fn start_debug_monitor(&mut self) -> anyhow::Result<()> {
        let (point_sender, mut point_receiver) = mpsc::channel(32);
        let (disconnect_sender, _) = mpsc::channel(32);
        self.interface
            .start_monitor(point_sender, disconnect_sender)
            .await?;

        tokio::spawn(async move {
            while let Some(points) = point_receiver.recv().await {
                // debug!("receive:{}", points.get_data());
                debug!("receive:{:?}", points.get_dt());
            }
        });
        Ok(())
    }
    pub async fn stop_debug_monitor(&mut self) -> anyhow::Result<()> {
        self.interface.stop_monitor().await?;

        Ok(())
    }
}

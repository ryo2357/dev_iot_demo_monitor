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
        let interface = DemoCpb16Interface::create_from_config(config).await?;
        Ok(Self { interface })
    }

    pub async fn single_data_collection(&mut self) -> anyhow::Result<()> {
        let (point_sender, mut point_receiver) = mpsc::channel(32);
        self.interface.start_moniter(point_sender).await?;
        match point_receiver.recv().await {
            Some(t) => debug!("receive:{}", t.get_data()),
            None => debug!("sender drop before send data"),
        }
        Ok(())
    }
}

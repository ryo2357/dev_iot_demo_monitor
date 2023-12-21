use influxdb2::models::DataPoint;
use log::warn;
use tokio::task::JoinHandle;
use tokio::time::Interval;
use tokio::{net::unix::pipe::Receiver, sync::mpsc};

mod config;
mod data_manager;
mod interface;

use config::DemoMachineConfig;
use data_manager::DemoMachineReceiveData;
use data_manager::DemoMachineStatus;
use interface::DemoMachineInterface;

#[derive(Debug, PartialEq)]
enum CollecterState {
    Stopping,
    Collecting,
}

pub struct DemoMachineCollecter {
    sender: mpsc::Sender<Vec<DataPoint>>,
    // config: DemoMachineConfig,
    interface: DemoMachineInterface,
    state: CollecterState,
    interface_hundle: Option<JoinHandle<()>>,
    manager_hundle: Option<JoinHandle<()>>,
}

impl DemoMachineCollecter {
    pub async fn create_from_env() -> anyhow::Result<(Self, mpsc::Receiver<Vec<DataPoint>>)> {
        let (tx, rx) = mpsc::channel(32);
        let config = DemoMachineConfig::create_from_env()?;
        let interface = DemoMachineInterface::create_from_config(config).await?;
        Ok((
            Self {
                sender: tx,
                interface,
                state: CollecterState::Stopping, // interface,
                interface_hundle: None,
                manager_hundle: None,
            },
            rx,
        ))
    }

    pub async fn start_data_collection(&mut self, interval: u64) -> anyhow::Result<()> {
        if self.state != CollecterState::Stopping {
            warn!(
                "start_data_collection can not execute: state = {:?}",
                self.state
            );
            anyhow::bail!(
                "start_data_collection can not execute: state = {:?}",
                self.state
            )
        }

        let (data_revever, interface_hundle) = self.interface.start_moniter(interval).await?;
        self.interface_hundle = Some(interface_hundle);
        self.state = CollecterState::Collecting;
        let sender = self.sender.clone();
        let conver_hundle = tokio::spawn(async move {
            // データ変換スレッドを作成する
        });
        self.manager_hundle = Some(conver_hundle);

        let hundle = tokio::spawn(async move {});
        Ok(())
    }

    pub fn stop_data_collection(&mut self) -> anyhow::Result<()> {
        if self.state != CollecterState::Collecting {
            warn!(
                "stop_data_collection can not execute: state = {:?}",
                self.state
            );
            anyhow::bail!(
                "stop_data_collection can not execute: state = {:?}",
                self.state
            )
        }
        self.state = CollecterState::Stopping;
        self.interface_hundle = None;
        self.manager_hundle = None;

        Ok(())
    }
}

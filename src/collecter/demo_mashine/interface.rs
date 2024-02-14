use chrono::Local;
use log::{debug, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use super::config::DemoMachineConfig;
use super::data_manager::DemoMachineReceiveData;
use super::data_manager::DemoMachineStatus;

pub struct DemoMachineInterface {
    config: DemoMachineConfig,
    thread: Option<CollecterThread>,
}
impl DemoMachineInterface {
    pub async fn create_from_config(config: DemoMachineConfig) -> anyhow::Result<Self> {
        // コンフィグからインターフェイスを作成。動作チェック
        let mut stream = TcpStream::connect(&config.get_address()).await?;
        stream.write_all(&config.get_check_command()).await?;
        let mut buf = [0; 4];
        let n = stream.read(&mut buf).await?;
        let res = std::str::from_utf8(&buf[..n])
            .unwrap()
            .trim_end_matches("\r\n")
            .to_string();
        debug!("チェックコマンドのレスポンス:{:?}", res);

        if res == config.get_check_response() {
            debug!("正しい機種");
        } else {
            debug!("想定外の機種");
            return Err(anyhow::anyhow!("diffelent plc"));
        }

        Ok(Self {
            config,
            thread: None,
        })
    }

    pub async fn start_moniter(
        &mut self,
        tx: mpsc::Sender<DemoMachineReceiveData>,
    ) -> anyhow::Result<()> {
        if self.thread.is_some() {
            anyhow::bail!("already started moniter in DemoMachineInterface::start_moniter")
        }
        let collecter_thread = CollecterThread::start(tx, self.config.clone()).await?;
        self.thread = Some(collecter_thread);
        debug!("DemoMachineInterface collect start");
        Ok(())
    }

    pub async fn stop_moniter(&mut self) -> anyhow::Result<()> {
        if let Some(thread) = self.thread.take() {
            thread.stop().await?;
        } else {
            anyhow::bail!("not start collect in DemoMachineInterface::stop_moniter")
        }
        debug!("DemoMachineInterface collect stop");
        Ok(())
    }

    pub fn is_monitoring(&self) -> bool {
        self.thread.is_some()
    }
}
impl Drop for DemoMachineInterface {
    fn drop(&mut self) {
        task::block_in_place(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if self.thread.is_some() {
                    self.stop_moniter().await.unwrap();
                }
            });
        });
    }
}
struct CollecterThread {
    collecter_thread: JoinHandle<()>,
    stop_sender: mpsc::Sender<()>,
}
impl CollecterThread {
    async fn start(
        tx: mpsc::Sender<DemoMachineReceiveData>,
        config: DemoMachineConfig,
    ) -> anyhow::Result<Self> {
        let (stop_sender, mut stop_receiver) = mpsc::channel(32);
        let mut stream = TcpStream::connect(&config.get_address()).await?;
        // モニタの登録処理
        stream.write_all(&config.get_set_moniter_command()).await?;
        let mut buf = [0; 5];
        let n = stream.read(&mut buf).await?;
        let res = std::str::from_utf8(&buf[..n])
            .unwrap()
            .trim_end_matches("\r\n")
            .to_string();
        match res.as_str() {
            "OK" => {}
            "E0" => return Err(anyhow::anyhow!("モニタ登録失敗：デバイス番号異常")),
            "E1" => return Err(anyhow::anyhow!("モニタ登録失敗：コマンド異常")),
            t => return Err(anyhow::anyhow!("モニタ登録失敗：想定外の返り値:{}", t)),
        }
        let command = config.get_monitor_readout_command();
        let mut state = DemoMachineState::create_from_config(&config);
        let mut interval = state.get_interval();

        let collecter_thread = tokio::spawn(async move {
            let mut next_loop_start_time = Instant::now();
            loop {
                next_loop_start_time += Duration::from_millis(interval);
                let now = Instant::now();
                if next_loop_start_time > now {
                    tokio::time::sleep(next_loop_start_time - now).await;
                } else {
                    next_loop_start_time = now;
                }

                tokio::select! {
                    _ = stop_receiver.recv() => {
                        break;
                    }
                    _ = tokio::time::sleep(next_loop_start_time - now) =>{
                        let result: anyhow::Result<()> = async {
                            stream.write_all(&command).await?;

                            // NOTE:データ点数が多くなった場合、バッファサイズを大きくする必要がある
                            let mut buf = [0; 1024];
                            let n = stream.read(&mut buf).await?;
                            let dt = Local::now();
                            let res = std::str::from_utf8(&buf[..n])
                                .unwrap()
                                .trim_end_matches("\r\n")
                                .to_string();

                            // NOTE:想定外のデータについてのハンドリングが必要
                            let recceive_data = DemoMachineReceiveData::create(dt, res)?;

                            let now_status = recceive_data.get_status();
                            if state.get_status() != now_status {
                                state.set_status(now_status);
                                interval = state.get_interval();
                            }

                            tx.send(recceive_data).await?;
                            Ok(())
                        }.await;

                        // recceive_data等のエラーハンドリング
                        if let Err(err) = result {
                            warn!("Error: {}", err);
                        }
                    }
                }
            }
        });

        Ok(Self {
            collecter_thread,
            stop_sender,
        })
    }

    async fn stop(self) -> anyhow::Result<()> {
        self.stop_sender.send(()).await?;
        // 完了を待つ処理
        self.collecter_thread.await?;
        Ok(())
    }
}

struct DemoMachineState {
    status: DemoMachineStatus,
    monitor_interval: u64,
    interval_when_machine_stop: u64,
}

impl DemoMachineState {
    fn create_from_config(config: &DemoMachineConfig) -> Self {
        Self {
            status: DemoMachineStatus::Stopping,
            monitor_interval: config.get_monitor_interval(),
            interval_when_machine_stop: config.get_interval_when_machine_stop(),
        }
    }

    fn get_status(&self) -> DemoMachineStatus {
        self.status.to_owned()
    }

    fn get_interval(&self) -> u64 {
        #[allow(unreachable_patterns)]
        match self.status {
            DemoMachineStatus::Stopping => self.interval_when_machine_stop.to_owned(),
            DemoMachineStatus::Running => self.monitor_interval.to_owned(),
            _ => self.interval_when_machine_stop.to_owned(),
        }
    }

    fn set_status(&mut self, status: DemoMachineStatus) {
        self.status = status
    }
}

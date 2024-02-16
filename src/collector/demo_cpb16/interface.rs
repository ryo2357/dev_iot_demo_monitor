use chrono::Local;
use log::{debug, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use super::config::DemoCpb16Config;
use super::data_manager::DemoCpb16ReceiveData;
use super::data_manager::DemoCpb16Status;

pub struct DemoCpb16Interface {
    config: DemoCpb16Config,
    thread: Option<ConnectionThread>,
}
impl DemoCpb16Interface {
    pub async fn create_from_config(config: DemoCpb16Config) -> anyhow::Result<Self> {
        // コンフィグからインターフェイスを作成。動作チェック
        let mut stream = TcpStream::connect(&config.get_address()).await?;
        debug!(
            "To:{:?},command:{:?} ",
            &config.get_address(),
            &config.get_check_command()
        );
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
            return Err(anyhow::anyhow!("different plc"));
        }

        Ok(Self {
            config,
            thread: None,
        })
    }

    pub async fn start_monitor(
        &mut self,
        data_sender: mpsc::Sender<DemoCpb16ReceiveData>,
        disconnect_sender: mpsc::Sender<()>,
    ) -> anyhow::Result<()> {
        if self.thread.is_some() {
            anyhow::bail!("already started monitor in DemoCpb16Interface::start_monitor")
        }
        let connection_thread =
            ConnectionThread::start(data_sender, disconnect_sender, self.config.clone()).await?;
        self.thread = Some(connection_thread);
        debug!("DemoCpb16Interface collect start");
        Ok(())
    }

    pub async fn stop_monitor(&mut self) -> anyhow::Result<()> {
        if let Some(thread) = self.thread.take() {
            thread.stop().await?;
        } else {
            anyhow::bail!("not start collect in DemoCpb16Interface::stop_moniter")
        }
        debug!("DemoCpb16Interface collect stop");
        Ok(())
    }

    pub fn is_monitoring(&self) -> bool {
        self.thread.is_some()
    }
}
impl Drop for DemoCpb16Interface {
    fn drop(&mut self) {
        task::block_in_place(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if self.thread.is_some() {
                    self.stop_monitor().await.unwrap();
                }
            });
        });
    }
}
struct ConnectionThread {
    connection_thread: JoinHandle<()>,
    stop_sender: mpsc::Sender<()>,
}
impl ConnectionThread {
    async fn start(
        data_sender: mpsc::Sender<DemoCpb16ReceiveData>,
        disconnect_sender: mpsc::Sender<()>,
        config: DemoCpb16Config,
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
        let mut state = DemoCpb16State::create_from_config(&config);
        let mut interval = state.get_interval();

        let connection_thread = tokio::spawn(async move {
            let mut next_loop_start_time = Instant::now();
            let mut duration: Duration;
            loop {
                let now = Instant::now();
                if next_loop_start_time > now {
                    duration = next_loop_start_time - now;
                } else {
                    duration = Duration::from_millis(0);
                }

                tokio::select! {
                    _ = stop_receiver.recv() => {
                        break;
                    }
                    _ = tokio::time::sleep(duration) =>{
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
                            let recceive_data = DemoCpb16ReceiveData::create(dt, res)?;

                            let now_status = recceive_data.get_status();
                            if state.get_status() != now_status {
                                state.set_status(now_status);
                                interval = state.get_interval();
                            }

                            data_sender.send(recceive_data).await?;
                            Ok(())
                        }.await;

                        // receive_data等のエラーハンドリング
                        if let Err(err) = result {
                            warn!("Error: {}", err);
                            disconnect_sender.send(()).await.unwrap();
                        }
                    }
                }

                next_loop_start_time += Duration::from_millis(interval);
            }
        });

        Ok(Self {
            connection_thread,
            stop_sender,
        })
    }

    async fn stop(self) -> anyhow::Result<()> {
        self.stop_sender.send(()).await?;
        // 完了を待つ処理
        self.connection_thread.await?;
        Ok(())
    }
}

struct DemoCpb16State {
    status: DemoCpb16Status,
    monitor_interval: u64,
    interval_when_machine_stop: u64,
}

impl DemoCpb16State {
    fn create_from_config(config: &DemoCpb16Config) -> Self {
        Self {
            status: DemoCpb16Status::Stopping,
            monitor_interval: config.get_monitor_interval(),
            interval_when_machine_stop: config.get_interval_when_machine_stop(),
        }
    }

    fn get_status(&self) -> DemoCpb16Status {
        self.status.to_owned()
    }

    fn get_interval(&self) -> u64 {
        #[allow(unreachable_patterns)]
        match self.status {
            DemoCpb16Status::Stopping => self.interval_when_machine_stop.to_owned(),
            DemoCpb16Status::Running => self.monitor_interval.to_owned(),
            _ => self.interval_when_machine_stop.to_owned(),
        }
    }

    fn set_status(&mut self, status: DemoCpb16Status) {
        self.status = status
    }
}

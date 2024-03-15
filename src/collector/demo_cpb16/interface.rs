use chrono::Local;
use log::{debug, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration, Instant};

use super::config;
use super::config::DemoCpb16Config;
use super::data_manager::DemoCpb16ReceiveData;
use super::data_manager::DemoCpb16Status;

pub struct DemoCpb16Interface {
    config: DemoCpb16Config,
    is_checked: bool,
    thread: Option<ConnectionThread>,
}
impl DemoCpb16Interface {
    pub fn create_from_config(config: DemoCpb16Config) -> anyhow::Result<Self> {
        // コンフィグからインターフェイスを作成。動作チェック
        Ok(Self {
            config,
            is_checked: false,
            thread: None,
        })
    }

    pub async fn check_connection(&mut self) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(&self.config.get_address()).await?;
        debug!(
            "To:{:?},command:{:?} ",
            &self.config.get_address(),
            std::str::from_utf8(config::CHECK_COMMAND).unwrap()
        );
        stream.write_all(config::CHECK_COMMAND).await?;
        let mut buf = [0; 4];
        let n = stream.read(&mut buf).await?;
        let res = std::str::from_utf8(&buf[..n])
            .unwrap()
            .trim_end_matches("\r\n")
            .to_string();
        debug!("チェックコマンドのレスポンス:{:?}", res);

        if res == config::CHECK_RESPONSE {
            debug!("正しい機種");
        } else {
            debug!("想定外の機種");
            return Err(anyhow::anyhow!("different plc"));
        }
        self.is_checked = true;

        Ok(())
    }

    pub async fn start_monitor(
        &mut self,
        data_sender: mpsc::Sender<DemoCpb16ReceiveData>,
        disconnect_sender: mpsc::Sender<()>,
    ) -> anyhow::Result<()> {
        if self.thread.is_some() {
            anyhow::bail!("already started monitor in DemoCpb16Interface::start_monitor")
        }
        if !self.is_checked {
            self.check_connection().await?;
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
            anyhow::bail!("not start collect in DemoCpb16Interface::stop_monitor")
        }
        debug!("DemoCpb16Interface collect stop");
        Ok(())
    }

    pub fn is_monitoring(&self) -> bool {
        self.thread.is_some()
    }

    pub async fn set_time_now(&mut self) -> anyhow::Result<()> {
        if self.thread.is_some() {
            anyhow::bail!("already started monitor in DemoCpb16Interface::set_time_now")
        }
        if !self.is_checked {
            self.check_connection().await?;
        }
        let mut stream = TcpStream::connect(&self.config.get_address()).await?;

        let command = self.config.get_time_preference_command();
        debug!("command:{:?}", std::str::from_utf8(&command).unwrap());
        stream
            .write_all(&self.config.get_time_preference_command())
            .await?;
        let mut buf = [0; 4];
        let n = stream.read(&mut buf).await?;
        let res = std::str::from_utf8(&buf[..n])
            .unwrap()
            .trim_end_matches("\r\n");

        match res {
            config::OK_RESPONSE => {
                debug!("時刻設定成功");
            }

            config::COMMAND_ABNORMAL_RESPONSE => {
                debug!("コマンドエラー");
            }
            _ => {
                debug!("想定外のレスポンス:{:?}", res);
            }
        }

        Ok(())
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
        stream.write_all(config::SET_MONITOR_COMMAND).await?;
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
        let mut state = DemoCpb16State::new();
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
                            // let write_result = timeout(Duration::from_secs(10), stream.write_all(&command)).await;
                            // タイムアウト処理を行う必要がある
                            // stream.write_all(&command).await?;
                            let _ = timeout(Duration::from_secs(5), stream.write_all(config::MONITOR_READOUT_COMMAND)).await?;

                            // NOTE:データ点数が多くなった場合、バッファサイズを大きくする必要がある
                            let mut buf = [0; 1024];
                            // let n = stream.read(&mut buf).await?;
                            let n = match timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
                                Ok(Ok(n)) => n,
                                Ok(Err(e)) => anyhow::bail!("error in stream.read:{}",e),
                                Err(_) => anyhow::bail!("timeout in stream.read"),
                            };
                            let dt = Local::now();
                            let res = std::str::from_utf8(&buf[..n])
                                .unwrap()
                                .trim_end_matches("\r\n")
                                .to_string();

                            // NOTE:想定外のデータについてのハンドリングが必要
                            let receive_data = DemoCpb16ReceiveData::create(dt, res)?;

                            let now_status = receive_data.get_status();
                            if state.get_status() != now_status {
                                state.set_status(now_status);
                                interval = state.get_interval();
                            }

                            data_sender.send(receive_data).await?;
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
    fn new() -> Self {
        Self {
            status: DemoCpb16Status::Stopping,
            monitor_interval: config::MONITOR_INTERVAL,
            interval_when_machine_stop: config::INTERVAL_WHEN_MACHINE_STOP,
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

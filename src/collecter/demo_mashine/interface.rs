use chrono::Local;
use log::{debug, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use super::DemoMachineConfig;
use super::DemoMachineReceiveData;
use super::DemoMachineStatus;

pub struct DemoMachineInterface {
    config: DemoMachineConfig,
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

        Ok(Self { config })
    }

    pub async fn start_moniter(
        &mut self,
    ) -> anyhow::Result<(mpsc::Receiver<DemoMachineReceiveData>, JoinHandle<()>)> {
        let (tx, rx) = mpsc::channel(32);

        let mut stream = TcpStream::connect(&self.config.get_address()).await?;
        stream
            .write_all(&self.config.get_set_moniter_command())
            .await?;
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
            _ => return Err(anyhow::anyhow!("モニタ登録失敗：想定外の返り値")),
        }

        let mut state = DemoMachineState::create_from_config(&self.config);
        let mut interval = state.get_interval();

        let command = self.config.get_monitor_readout_command();
        let mut next_loop_start_time = Instant::now();

        let hundle = tokio::spawn(async move {
            loop {
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

                    // インターバルの調整
                    next_loop_start_time += Duration::from_millis(interval);
                    let now = Instant::now();
                    if next_loop_start_time > now {
                        tokio::time::sleep(next_loop_start_time - now).await;
                    } else {
                        next_loop_start_time = now;
                    }

                    Ok(())
                }
                .await;

                // recceive_data等のエラーハンドリング
                if let Err(err) = result {
                    warn!("Error: {}", err);
                }
            }
        });

        Ok((rx, hundle))
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

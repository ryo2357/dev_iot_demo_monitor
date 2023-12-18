use chrono::{DateTime, Local};
use log::{debug, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use super::DemoMachineConfig;

pub struct ReceicveData {
    dt: DateTime<Local>,
    data: String,
}
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

    // pub async fn check_connection(&mut self) -> anyhow::Result<()> {
    //     let mut stream = TcpStream::connect(self.config.get_address()).await?;
    //     stream.write_all(&self.config.get_check_command()).await?;

    //     let mut buf = [0; 4];
    //     let n = stream.read(&mut buf).await?;
    //     let res = std::str::from_utf8(&buf[..n])
    //         .unwrap()
    //         .trim_end_matches("\r\n")
    //         .to_string();
    //     debug!("チェックコマンドのレスポンス:{:?}", res);

    //     if res == self.config.get_check_response() {
    //         self.is_check_connection = true;
    //         debug!("正しい機種");
    //         Ok(())
    //     } else {
    //         self.is_check_connection = false;
    //         debug!("想定外の機種");
    //         Err(anyhow::anyhow!("diffelent plc"))
    //     }
    // }

    pub async fn start_moniter(
        &mut self,
        interval: u64,
    ) -> anyhow::Result<(mpsc::Receiver<ReceicveData>, JoinHandle<()>)> {
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

        let command = self.config.get_monitor_readout_command();
        let mut next_loop_start_time = Instant::now();

        let hundle = tokio::spawn(async move {
            loop {
                let result: anyhow::Result<()> = async {
                    next_loop_start_time += Duration::from_millis(interval);
                    stream.write_all(&command).await?;

                    // NOTE:データ点数が多くなった場合、バッファサイズを大きくする必要がある
                    let mut buf = [0; 1024];
                    let n = stream.read(&mut buf).await?;
                    let res = std::str::from_utf8(&buf[..n])
                        .unwrap()
                        .trim_end_matches("\r\n")
                        .to_string();
                    let recceive_data = ReceicveData {
                        dt: Local::now(),
                        data: res,
                    };

                    tx.send(recceive_data).await?;
                    let now = Instant::now();
                    if next_loop_start_time > now {
                        tokio::time::sleep(next_loop_start_time - now).await;
                    }

                    Ok(())
                }
                .await;

                if let Err(err) = result {
                    // Print or handle the error as needed
                    warn!("Error: {}", err);
                }
            }
        });

        Ok((rx, hundle))
    }

    // pub async fn set_moniter(&mut self) -> anyhow::Result<bool> {
    //     Ok(true)
    // }

    // pub async fn get_single_moniter_response(&mut self) -> anyhow::Result<String> {
    //     Ok("Ok".into())
    // }

    // pub async fn start_moniter(&mut self) -> anyhow::Result<mpsc::Sender<()>> {
    //     Ok("Ok".into())
    // }

    // pub async fn stop_moniter(&mut self) -> anyhow::Result<mpsc::Sender<()>> {
    //     Ok("Ok".into())
    // }
}

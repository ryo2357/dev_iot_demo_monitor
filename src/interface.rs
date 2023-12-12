use chrono::{DateTime, Local};
use log::debug;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

pub struct DemoMachineStatusReceiveData {
    pub data: Vec<u8>,
    pub count: u32,
    pub notify: u32,
    pub user: u32,
}
pub struct DemoMachineStatusConfig {
    address: String,
    check_command: Vec<u8>,
    check_response: String,
    collecter_command: Vec<u8>,
    set_moniter_command: Vec<u8>,
    monitor_readout_command: Vec<u8>,
}
impl DemoMachineStatusConfig {
    pub fn create_from_env() -> anyhow::Result<Self> {
        let address = std::env::var("DemoMachineStatusConfigAddress")?;
        // let mut check_command: Vec<u8> =
        //     std::env::var("DemoMachineStatusConfigCheckCommand")?.into();
        // check_command.push(13); // 改行コード
        let check_command: Vec<u8> = "?K\r".into();
        let check_response = std::env::var("DemoMachineStatusConfigCheckResponse")?;

        // let mut collecter_command: Vec<u8> =
        //     std::env::var("DemoMachineStatusConfigCollecterCommand")?.into();

        let collecter_command: Vec<u8> = "RDS DM1000.U 101\r".into();
        let set_moniter_command: Vec<u8> =
            "MWS DM1000.U DM1001.L DM1002.U DM1003.U DM1004.U DM1008.U DM1009.U DM1100.U\r".into();
        let monitor_readout_command: Vec<u8> = "MWR\r".into();

        Ok(Self {
            address,
            check_command,
            check_response,
            collecter_command,
            set_moniter_command,
            monitor_readout_command,
        })
    }
    pub fn get_address(&self) -> String {
        self.address.to_owned()
    }

    pub fn get_check_command(&self) -> Vec<u8> {
        self.check_command.to_owned()
    }

    pub fn get_check_response(&self) -> String {
        self.check_response.to_owned()
    }

    pub fn get_collecter_command(&self) -> Vec<u8> {
        self.collecter_command.to_owned()
    }
    pub fn get_set_moniter_command(&self) -> Vec<u8> {
        self.set_moniter_command.to_owned()
    }
    pub fn get_monitor_readout_command(&self) -> Vec<u8> {
        self.monitor_readout_command.to_owned()
    }
}

pub struct DemoMachineStatus {
    sender: mpsc::Sender<DemoMachineStatusReceiveData>,
    config: DemoMachineStatusConfig,
}
impl DemoMachineStatus {
    // pub fn create(
    //     config: DemoMachineStatusConfig,
    // ) -> anyhow::Result<(Self, mpsc::Receiver<DemoMachineStatusReceiveData>)> {
    //     let (tx, rx) = mpsc::channel(32);
    //     Ok((Self { sender: tx, config }, rx))
    // }

    pub fn create_from_env() -> anyhow::Result<(Self, mpsc::Receiver<DemoMachineStatusReceiveData>)>
    {
        let (tx, rx) = mpsc::channel(32);
        let config = DemoMachineStatusConfig::create_from_env()?;
        Ok((Self { sender: tx, config }, rx))
    }

    pub async fn check_connection(&mut self) -> anyhow::Result<bool> {
        // debug!("{:?}にストリーム生成", &self.config.get_address());
        let mut stream = TcpStream::connect(&self.config.get_address()).await?;
        // debug!("ストリーム生成完了");
        stream.write_all(&self.config.get_check_command()).await?;
        // debug!("データを送信");

        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await?;

        // debug!("{:?}バイトのデータを受信", n);
        // debug!("{:?}", &buf[..n]);
        // [53, 53, 13, 10] ⇒ 5 5 CR LF
        let res = std::str::from_utf8(&buf[..n])
            .unwrap()
            .trim_end_matches("\r\n")
            .to_string();
        debug!("チェックコマンドのレスポンス：response:{:?}", res);

        if res == self.config.get_check_response() {
            debug!("正しい機種");
            Ok(true)
        } else {
            debug!("想定外の機種");
            Ok(false)
        }
    }

    pub async fn test_reponse(&mut self) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(&self.config.get_address()).await?;
        let command = &self.config.get_collecter_command();
        stream.write_all(command).await?;

        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await?;

        let res = std::str::from_utf8(&buf[..n])
            .unwrap()
            .trim_end_matches("\r\n")
            .to_string();
        let res: Vec<&str> = res.split(' ').collect();
        let default_value = "データが存在しない";

        debug!(
            "レスポンス：DM1000:{:?}",
            res.first().unwrap_or(&default_value)
        );

        debug!(
            "レスポンス：DM1001:{:?}",
            res.get(1).unwrap_or(&default_value)
        );

        debug!(
            "レスポンス：DM1100:{:?}",
            res.get(100).unwrap_or(&default_value)
        );

        Ok(())
    }

    pub async fn test_reponse_loop(&mut self) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(&self.config.get_address()).await?;
        let command = &self.config.get_collecter_command();

        let mut next_loop_start_time = Instant::now();
        for _ in 0..100 {
            next_loop_start_time += Duration::from_millis(50);

            stream.write_all(command).await?;

            let mut buf = [0; 1024];
            let n = stream.read(&mut buf).await?;

            let res = std::str::from_utf8(&buf[..n])
                .unwrap()
                .trim_end_matches("\r\n")
                .to_string();
            let res: Vec<&str> = res.split(' ').collect();
            let default_value = "データが存在しない";

            let dt: DateTime<Local> = Local::now();

            debug!(
                "{:?}：DM1000:{:?},DM1100:{:?}",
                dt,
                res.first().unwrap_or(&default_value),
                res.get(100).unwrap_or(&default_value)
            );

            let now = Instant::now();
            if next_loop_start_time > now {
                tokio::time::sleep(next_loop_start_time - now).await;
            }
        }

        Ok(())
    }
    pub async fn monitor_test(&mut self) -> anyhow::Result<()> {
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
        debug!("セットコマンドのレスポンス：response:{:?}", res);
        let command = &self.config.get_monitor_readout_command();

        let mut next_loop_start_time = Instant::now();
        for _ in 0..100 {
            next_loop_start_time += Duration::from_millis(50);

            stream.write_all(command).await?;

            let mut buf = [0; 1024];
            let n = stream.read(&mut buf).await?;

            let res = std::str::from_utf8(&buf[..n])
                .unwrap()
                .trim_end_matches("\r\n")
                .to_string();
            let res: Vec<&str> = res.split(' ').collect();
            let default_value = "データが存在しない";

            let dt: DateTime<Local> = Local::now();

            debug!(
                "{:?}：DM1000:{:?},DM1100:{:?}",
                dt,
                res.first().unwrap_or(&default_value),
                res.get(7).unwrap_or(&default_value)
            );
            // debug!("{:?}：{:?}", dt, res);
            let now = Instant::now();
            if next_loop_start_time > now {
                tokio::time::sleep(next_loop_start_time - now).await;
            }
        }

        Ok(())
    }
    pub fn start_data_collection(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn stop_data_collection(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

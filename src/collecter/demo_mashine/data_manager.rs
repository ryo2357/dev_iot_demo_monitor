use chrono::{DateTime, Local};
use influxdb2::models::DataPoint;
use tokio::sync::mpsc;
use tokio::task;

pub const SET_MONITER_COMMAND: &[u8] =
    b"MWS DM1000.U DM1001.L DM1002.U DM1003.U DM1004.U DM1008.U DM1009.U DM1100.U\r";

const RESPONSE_LENGTH: usize = 50;
const SEND_CHUNK_SIZE: usize = 50;
const OPERATING_DATA_INTERVAL_SEC: u32 = 5;

pub struct DemoMachineDataManager {
    sender: mpsc::Sender<Vec<DataPoint>>,
    last_machine_status: DemoMachineStatus,
    send_chunk_size: usize,

    // 保存周期の長い稼働情報　5s毎のデータを保存
    // 機械停止中もデータベースに保存
    operating_data: Vec<DataPoint>,
    last_operating_data_time: DateTime<Local>,
    operating_data_interval_sec: u32,

    // 全データを保存
    // configのintervalに等しい
    sensor_data: Vec<DataPoint>,
    // last_sensor_data_time: DateTime<Local>,
}

impl DemoMachineDataManager {
    pub fn create(sender: mpsc::Sender<Vec<DataPoint>>) -> Self {
        let dt = Local::now();
        // TODO:定数はConfigに
        Self {
            sender,
            last_machine_status: DemoMachineStatus::Stopping,
            send_chunk_size: SEND_CHUNK_SIZE,
            operating_data: Vec::<DataPoint>::new(),
            last_operating_data_time: dt,
            operating_data_interval_sec: OPERATING_DATA_INTERVAL_SEC,
            sensor_data: Vec::<DataPoint>::new(),
            // last_sensor_data_time: dt,
        }
    }

    pub async fn recceive_response(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        #[allow(unreachable_patterns)]
        match self.last_machine_status {
            DemoMachineStatus::Running => match data.get_status() {
                DemoMachineStatus::Running => self.recceive_in_runnning(data).await?,
                DemoMachineStatus::Stopping => self.recceive_to_stopping(data).await?,
                _ => anyhow::bail!("受信データのMachineStatusが不正"),
            },
            DemoMachineStatus::Stopping => match data.get_status() {
                DemoMachineStatus::Running => self.recceive_to_runnning(data).await?,
                DemoMachineStatus::Stopping => self.recceive_in_stopping(data).await?,
                _ => {
                    anyhow::bail!("受信データのMachineStatusが不正")
                }
            },
            _ => anyhow::bail!("データマネージャーのMachineStatusが不正"),
        }
        Ok(())
    }
    // 内部関数
    // 稼働状態での分岐
    async fn recceive_in_stopping(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        if self.shoud_set_operating_data(data.get_dt()) {
            self.set_operation_data(data).await?;
        }
        Ok(())
    }
    async fn recceive_in_runnning(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        if self.shoud_set_operating_data(data.get_dt()) {
            self.set_operating_and_sensor(data).await?;
        } else {
            self.set_sensor_data(data).await?;
        }

        Ok(())
    }
    async fn recceive_to_stopping(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        if self.shoud_set_operating_data(data.get_dt()) {
            self.set_operation_data(data).await?;
        }
        // 保持しているセンサー情報を一旦送信
        self.send_sensor_data().await?;
        Ok(())
    }
    async fn recceive_to_runnning(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        // 切り替え時に特殊な処理を行わないので
        // recceive_in_runnningと同じになる
        if self.shoud_set_operating_data(data.get_dt()) {
            self.set_operating_and_sensor(data).await?;
        } else {
            self.set_sensor_data(data).await?;
        }
        Ok(())
    }
    // 判定メソッド
    fn shoud_set_operating_data(&self, receive_dt: DateTime<Local>) -> bool {
        let duration = receive_dt - self.last_operating_data_time;
        let duration_sec = duration.num_seconds() as u32;
        if duration_sec < self.operating_data_interval_sec {
            return false;
        }
        true
    }

    // set data
    // DemoMachineReceiveDataを消費する
    async fn set_operation_data(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        let new_dt = data.get_dt();
        let operation_point = data.parse_operation_data()?;
        self.operating_data.push(operation_point);
        self.last_operating_data_time = new_dt;

        if self.operating_data.len() >= self.send_chunk_size {
            self.send_operating_data().await?;
        }
        Ok(())
    }
    async fn set_sensor_data(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        let sensor_point = data.parse_sensor_data()?;
        self.sensor_data.push(sensor_point);

        if self.sensor_data.len() >= self.send_chunk_size {
            self.send_sensor_data().await?;
        }

        Ok(())
    }
    async fn set_operating_and_sensor(
        &mut self,
        data: DemoMachineReceiveData,
    ) -> anyhow::Result<()> {
        let new_dt = data.get_dt();
        let operation_point = data.parse_operation_data()?;
        self.operating_data.push(operation_point);
        self.last_operating_data_time = new_dt;

        let sensor_point = data.parse_sensor_data()?;
        self.sensor_data.push(sensor_point);

        if self.operating_data.len() >= self.send_chunk_size {
            self.send_operating_data().await?;
        }
        if self.sensor_data.len() >= self.send_chunk_size {
            self.send_sensor_data().await?;
        }

        Ok(())
    }

    // send data
    // TODO:Drop時に実行する
    async fn send_operating_data(&mut self) -> anyhow::Result<()> {
        let send_data = std::mem::take(&mut self.operating_data);
        self.sender.send(send_data).await?;
        Ok(())
    }
    async fn send_sensor_data(&mut self) -> anyhow::Result<()> {
        // 下記コードよりも軽く実装されているはず
        // let send_data = self.sensor_data.clone();
        // self.sensor_data.clear();
        let send_data = std::mem::take(&mut self.sensor_data);
        self.sender.send(send_data).await?;

        Ok(())
    }
}

impl Drop for DemoMachineDataManager {
    // NOTE:Dropトレイト内のエラー処理はどうする
    fn drop(&mut self) {
        task::block_in_place(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // 何か非同期的な処理を行う
                println!("data_manager drop start");
                if !self.operating_data.is_empty() {
                    self.send_operating_data().await.unwrap();
                }
                if !self.sensor_data.is_empty() {
                    self.send_sensor_data().await.unwrap();
                }
                println!("data_manager drop end");
            });
        });
    }
}

pub struct DemoMachineReceiveData {
    dt: DateTime<Local>,
    data: String,
    status: DemoMachineStatus,
}
impl DemoMachineReceiveData {
    pub fn create(dt: DateTime<Local>, data: String) -> anyhow::Result<Self> {
        // TODO:Stringが短い場合(受信データが不正な場合)のエラーハンドリング
        if data.len() != RESPONSE_LENGTH {
            anyhow::bail!("データ長が{:?}と異なる:{:?}", RESPONSE_LENGTH, data.len())
        }

        // TODO:実態に合わせた判定式を作成
        let status = match &data[4..6] {
            "DM" => DemoMachineStatus::Running,
            _ => DemoMachineStatus::Stopping,
        };
        Ok(Self { dt, data, status })
    }

    pub fn get_status(&self) -> DemoMachineStatus {
        self.status.clone()
    }
    pub fn get_dt(&self) -> DateTime<Local> {
        self.dt
    }
    // pub fn move_inner(self) -> (DateTime<Local>, String) {
    //     (self.dt, self.data)
    // }

    fn parse_operation_data(&self) -> anyhow::Result<DataPoint> {
        let time = match self.dt.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let res: Vec<&str> = self.data.split(' ').collect();

        let is_running = matches!(self.status, DemoMachineStatus::Running);
        // let is_running = match self.status {
        //     DemoMachineStatus::Running => true,
        //     _ => false,
        // };

        let tempureture_1: i64 = match res.get(7) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let tempureture_2: i64 = match res.get(8) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let tempureture_3: i64 = match res.get(9) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        // bool,i64,f64,String,&strが可能
        let operation_point = DataPoint::builder("demo_machine")
            .tag("info_type", "operation")
            .field("is_running", is_running)
            .field("tempureture_1", tempureture_1)
            .field("tempureture_2", tempureture_2)
            .field("tempureture_3", tempureture_3)
            .timestamp(time)
            .build()?;

        Ok(operation_point)
    }
    fn parse_sensor_data(&self) -> anyhow::Result<DataPoint> {
        let time = match self.dt.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let res: Vec<&str> = self.data.split(' ').collect();

        let is_running = matches!(self.status, DemoMachineStatus::Running);
        // let is_running = match self.status {
        //     DemoMachineStatus::Running => true,
        //     _ => false,
        // };

        let tempureture_1: i64 = match res.get(7) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let tempureture_2: i64 = match res.get(8) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let tempureture_3: i64 = match res.get(9) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        // bool,i64,f64,String,&strが可能
        let sensor_point = DataPoint::builder("demo_machine")
            .tag("info_type", "sensor")
            .field("is_running", is_running)
            .field("tempureture_1", tempureture_1)
            .field("tempureture_2", tempureture_2)
            .field("tempureture_3", tempureture_3)
            .timestamp(time)
            .build()?;

        Ok(sensor_point)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum DemoMachineStatus {
    Running,
    Stopping,
}

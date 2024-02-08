use chrono::{DateTime, Local};
use influxdb2::models::DataPoint;
use log::{debug, error};
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;

//
pub const SET_MONITER_COMMAND: &[u8] =
    b"MWS DM1000.U DM1001.L DM1002.U DM1003.U DM1004.U DM1008.U DM1009.U DM1100.U\r";

// "40137 +0000000000 00000 00000 00000 00000 00000 34601"
// DM1002を稼働状況にする　⇒　DemoMachineReceiveData::create()で確認している
// 00000 : 停止流、00001 : 稼働中
//
// DM1000,DM1100 : 40ms毎に加算　オペレーションデータ
// DM1003：80ms毎and稼働時に加算　センサデータ
// DM1004：80ms毎and停止時に加算

const RESPONSE_LENGTH: usize = 53;

// sensor data 50ms × 50chunk = 2.5s
// 2.5秒毎に出力される
const SEND_CHUNK_SIZE: usize = 50;

// operating data 1s × 50chunk = 50s
// 50秒毎に出力される
// const OPERATING_DATA_INTERVAL_SEC: u32 = 5;
const OPERATING_DATA_INTERVAL_SEC: u32 = 1;

// point_senderがドロップされるとthreadは終了
// ⇒DemoMachineDataHundlerがドロップ
// ⇒端数データの送信処理
pub struct DemoMachineDataManager {
    thread: JoinHandle<()>,
}
impl DemoMachineDataManager {
    pub fn create(
        data_sender: mpsc::Sender<Vec<DataPoint>>,
        mut point_receiver: mpsc::Receiver<DemoMachineReceiveData>,
    ) -> anyhow::Result<Self> {
        let mut state = DemoMachineDataHundler::create(data_sender)?;

        let thread = tokio::spawn(async move {
            while let Some(data) = point_receiver.recv().await {
                match state.recceive_response(data).await {
                    Ok(()) => {}
                    Err(r) => {
                        // TODO:ここのエラーハンドリングは用検討
                        // プログラムを終了させてもよい？
                        error!(
                            "error in DemoMachineDataManager::recceive_response():{:?}",
                            r
                        )
                    }
                }
            }
        });

        Ok(Self { thread })
    }
    // 明示的にドロップさせる
    pub async fn wait_thread_finished(self) -> anyhow::Result<()> {
        debug!("wait thread finished");
        self.thread.await?;
        debug!("confirmed thread finished");
        Ok(())
    }
}

struct DemoMachineDataHundler {
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

impl DemoMachineDataHundler {
    fn create(sender: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<Self> {
        let dt = Local::now();
        // TODO:定数はConfigに
        Ok(Self {
            sender,
            last_machine_status: DemoMachineStatus::Stopping,
            send_chunk_size: SEND_CHUNK_SIZE,
            operating_data: Vec::<DataPoint>::new(),
            last_operating_data_time: dt,
            operating_data_interval_sec: OPERATING_DATA_INTERVAL_SEC,
            sensor_data: Vec::<DataPoint>::new(),
            // last_sensor_data_time: dt,
        })
    }

    async fn recceive_response(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        // debug!("recceive_response");
        // 5秒毎にデータ収集してる
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
        // debug!("recceive_in_stopping");
        if self.shoud_set_operating_data(data.get_dt()) {
            self.set_operation_data(data).await?;
        }
        Ok(())
    }
    async fn recceive_in_runnning(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        // debug!("recceive_in_runnning");
        if self.shoud_set_operating_data(data.get_dt()) {
            self.set_operating_and_sensor(data).await?;
        } else {
            self.set_sensor_data(data).await?;
        }

        Ok(())
    }
    async fn recceive_to_stopping(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        self.last_machine_status = DemoMachineStatus::Stopping;
        debug!("recceive_to_stopping");

        if self.shoud_set_operating_data(data.get_dt()) {
            self.set_operation_data(data).await?;
        }
        // 保持しているセンサー情報を一旦送信
        self.send_sensor_data().await?;
        Ok(())
    }
    async fn recceive_to_runnning(&mut self, data: DemoMachineReceiveData) -> anyhow::Result<()> {
        self.last_machine_status = DemoMachineStatus::Running;
        debug!("recceive_to_runnning");
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
    // NOTE:Drop時に実行する
    async fn send_operating_data(&mut self) -> anyhow::Result<()> {
        let send_data = std::mem::take(&mut self.operating_data);
        debug!("send_operating_data {} data", send_data.len());
        self.sender.send(send_data).await?;
        Ok(())
    }
    async fn send_sensor_data(&mut self) -> anyhow::Result<()> {
        let send_data = std::mem::take(&mut self.sensor_data);
        debug!("send_sensor_data {} data", send_data.len());
        self.sender.send(send_data).await?;

        Ok(())
    }
}

impl Drop for DemoMachineDataHundler {
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
            anyhow::bail!(
                "データ長が{:?}と異なる:{:?}:{:?}",
                RESPONSE_LENGTH,
                data.len(),
                data
            )
        }

        // TODO:実態に合わせた判定式を作成
        // DM1002を稼働状況のデバイスとしている
        let status = match &data[18..23] {
            "00001" => DemoMachineStatus::Running,
            "00000" => DemoMachineStatus::Stopping,
            _ => DemoMachineStatus::Stopping,
        };
        // debug!("{}", &data[18..22]);
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

        // 多分u32やがDataPointの型の都合上、i64にパースする必要がある
        let dm_1100: i64 = match res.get(7) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let dm_1000: i64 = match res.get(1) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        // bool,i64,f64,String,&strが可能
        let operation_point = DataPoint::builder("demo_machine")
            .tag("info_type", "operation")
            .field("is_running", is_running)
            .field("dm_1100", dm_1100)
            .field("dm_1000", dm_1000)
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

        // センサーデータは稼働中のみ取得するので不要
        // let is_running = matches!(self.status, DemoMachineStatus::Running);

        let dm_1003: i64 = match res.get(2) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_sensor_dataでエラー"),
        };

        let dm_1004: i64 = match res.get(3) {
            Some(t) => t.parse()?,
            None => anyhow::bail!("parse_sensor_dataでエラー"),
        };

        // bool,i64,f64,String,&strが可能
        let sensor_point = DataPoint::builder("demo_machine")
            .tag("info_type", "sensor")
            .field("tempureture_1", dm_1003)
            .field("tempureture_2", dm_1004)
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

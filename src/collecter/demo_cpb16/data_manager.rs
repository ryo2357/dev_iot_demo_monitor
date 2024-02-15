use std::os::unix::thread;

use chrono::{DateTime, Local, SecondsFormat, TimeZone};
use influxdb2::models::{data_point, DataPoint};
use log::{debug, error};
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;

//
pub const SET_MONITER_COMMAND: &[u8] = b"\
    MWS DM0.U DM50.U DM100.U DM102.U DM104.U DM106.U \
    DM10.U DM12.U DM14.U DM16.U DM18.U DM20.U \
    DM22.U DM24.U DM26.U DM28.U DM30.U DM32.U \
    DM34.U DM36.U DM38.U DM40.U DM42.U DM44.U\r";

// pub const SET_MONITER_COMMAND: &[u8] = b"MWS DM0.U DM50.U DM100.U DM102.U DM104.U DM106.U\r";
// DM1002を稼働状況にする　⇒　DemoCpb16ReceiveData::create()で確認している
// 00000 : 停止流、00001 : 稼働中
//
// DM0 : 現在の稼働の有無  00000 : 停止中、00001 : 稼働中
// DM50 : 稼働のユニークIＤ
//
// DM100 : 現在の稼働の生産数(袋)
// DM102 : 現在の稼働の不良生産数(袋)
// DM104 : 前回稼働の生産数(袋)
// DM106 : 前回稼働の不良生産数(袋)
// DM10～ : 現在の稼働の開始時間
//  - DM10 : 年
//  - DM12 : 月
//  - DM14 : 日
//  - DM16 : 時
//  - DM18 : 分
//  - DM20 : 秒
// DM22～ : １つ前の稼働の開始時間
//  - DM22 : 年
//  - DM24 : 月
//  - DM26 : 日
//  - DM28 : 時
//  - DM30 : 分
//  - DM32 : 秒
// DM34～ : １つ前の稼働の稼働終了
//  - DM34 : 年
//  - DM36 : 月
//  - DM38 : 日
//  - DM40 : 時
//  - DM42 : 分
//  - DM44 : 秒

const RESPONSE_LENGTH: usize = 143;
// 5×24+24-1=143

// sensor data 50ms × 50chunk = 2.5s
// 2.5秒毎に出力される
const SEND_CHUNK_SIZE: usize = 50;

// operating data 1s × 50chunk = 50s
// 50秒毎に出力される
// const OPERATING_DATA_INTERVAL_SEC: u32 = 5;
const OPERATING_DATA_INTERVAL_SEC: u32 = 1;

pub struct DemoCpb16DataManager {
    thread: Option<JoinHandle<DemoCpb16DataHundler>>,
    state: Option<DemoCpb16DataHundler>,
}
impl DemoCpb16DataManager {
    pub fn create(data_sender: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<Self> {
        let mut state: DemoCpb16DataHundler = DemoCpb16DataHundler::create(data_sender)?;

        Ok(Self {
            thread: None,
            state: Some(state),
        })
    }
    pub async fn create_thread(
        &mut self,
        mut point_receiver: mpsc::Receiver<DemoCpb16ReceiveData>,
    ) -> anyhow::Result<()> {
        if self.thread.is_some() {
            anyhow::bail!("thread is already created")
        }
        let mut state = self.state.take().unwrap();
        let thread = tokio::spawn(async move {
            while let Some(data) = point_receiver.recv().await {
                match state.recceive_response(data).await {
                    Ok(()) => {}
                    Err(r) => {
                        // TODO:ここのエラーハンドリングは用検討
                        error!("error in DemoCpb16DataManager::recceive_response():{:?}", r)
                    }
                }
            }
            state
        });
        self.thread = Some(thread);
        Ok(())
    }

    // PLCの接続が切れたらしたらstateはDemoCpb16DataManagerが保持
    pub async fn finish_thread(&mut self) -> anyhow::Result<()> {
        if self.thread.is_none() {
            anyhow::bail!("thread is none")
        }
        let Some(thread) = self.thread.take() else {
            anyhow::bail!("wait_thread_finishedでエラー")
        };
        let state = thread.await?;
        self.state = Some(state);
        Ok(())
    }
}
// データは１秒毎に収集
// 10秒毎に稼働状況を作成
// 稼働状況は1分毎に送信（6個）
// 稼働成果は動作が完了する度に送信（１個）

struct DemoCpb16OperationChunkData{
    operating_states_chunk_size: u32,
    operating_states_chunk_count: u32,
    chunk_start_production_count: u32,
    chunk_start_defect_count: u32,
    chunk_production: u32,
    chunk_defect: u32,
    chunk_work_second:i64
}

impl DemoCpb16OperationChunkData{
    fn new() -> Self{
        Self{
            operating_states_chunk_size: 10,
            operating_states_chunk_count: 0,
            chunk_start_production_count: 0,
            chunk_start_defect_count: 0,
            chunk_production: 0,
            chunk_defect: 0,
            chunk_work_second:0
        }
    } 

    fn push_running_data(&mut self,data:&DemoCpb16ReceiveState) -> anyhow::Result<Option<DataPoint>> {
        self.operating_states_chunk_count += 1;
        self.chunk_production = data.production_count - self.chunk_start_production_count;
        self.chunk_defect = data.defect_count - self.chunk_start_defect_count;
        self.chunk_work_second  += 1;

        if self.operating_states_chunk_count == 10{
            let data = self.make_working_data(data)?;
            Ok(Some(data))
        }else{
            Ok(None)
        }
    }

    fn push_stopping_data(&mut self,data:&DemoCpb16ReceiveState) -> anyhow::Result<Option<DataPoint>> {
        self.operating_states_chunk_count += 1;
        self.chunk_work_second  += 1;

        self.chunk_start_production_count = 0;
        self.chunk_start_defect_count= 0;

        if self.operating_states_chunk_count == 10{
            let data = self.make_working_data(data)?;
            Ok(Some(data))
        }else{
            Ok(None)
        }
    }

    fn recet_chunk_from_data(&mut self,data:&DemoCpb16ReceiveState){
        self.operating_states_chunk_count =0;
        self.chunk_work_second =0;
        self.chunk_defect = 0;
        self.chunk_production = 0;
        
        self.chunk_start_production_count = data.production_count;
        self.chunk_start_defect_count= data.defect_count;
    }
    // fn set_new_runnnig(&mut self,data:&DemoCpb16ReceiveState){
    //     self.chunk_start_production_count = 0;
    //     self.chunk_start_defect_count= 0;
    // }

    fn make_working_data(&mut self,data:&DemoCpb16ReceiveState)-> anyhow::Result<DataPoint>{
        let is_working = match data.status{
            DemoCpb16Status::Running => true,
            DemoCpb16Status::Stopping=> false,
        };
        let time = match data.receive_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("parse_operation_dataでエラー"),
        };

        let working_data = DataPoint::builder("demo_cpb16")
            .tag("info_type", "working")
            .field("is_working", is_working)
            .field("working_second", self.chunk_work_second)
            .field("production", self.chunk_production as i64)
            .field("defect", self.chunk_defect as i64)
            .timestamp(time)
            .build()?;

        self.recet_chunk_from_data(data);

        Ok(working_data)
    }
}
struct DemoCpb16DataHundler {
    sender: mpsc::Sender<Vec<DataPoint>>,
    last_machine_status: DemoCpb16Status,
    operating_states_chunk: DemoCpb16OperationChunkData,
    send_data_length:usize,
    operating_send_data: Vec<DataPoint>,
}

impl DemoCpb16DataHundler {
    fn create(sender: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<Self> {
        // TODO:定数はConfigに
        Ok(Self {
            sender,
            last_machine_status: DemoCpb16Status::Stopping,
            operating_states_chunk:DemoCpb16OperationChunkData::new(),
            send_data_length:6,
            operating_send_data: Vec::<DataPoint>::new(),
        })
    }

    async fn recceive_response(&mut self, data: DemoCpb16ReceiveData) -> anyhow::Result<()> {
        // debug!("recceive_response");
        // 5秒毎にデータ収集してる
        let state = DemoCpb16ReceiveState::new(data)?;
        #[allow(unreachable_patterns)]
        match self.last_machine_status {
            DemoCpb16Status::Running => match state.status {
                DemoCpb16Status::Running => self.recceive_in_runnning(state).await?,
                DemoCpb16Status::Stopping => self.recceive_to_stopping(state).await?,
                _ => anyhow::bail!("受信データのMachineStatusが不正"),
            },
            DemoCpb16Status::Stopping => match state.status {
                DemoCpb16Status::Running => self.recceive_to_runnning(state).await?,
                DemoCpb16Status::Stopping => self.recceive_in_stopping(state).await?,
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
    async fn push_send_data(&mut self, data_point: DataPoint) -> anyhow::Result<()> {
        self.operating_send_data.push(data_point);
        if self.operating_send_data.len() == self.send_data_length{
            let send_data = std::mem::replace(&mut self.operating_send_data, Vec::new());
            self.sender.send(send_data).await?;
        };
        Ok(())
    }

    async fn recceive_in_stopping(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        match  self.operating_states_chunk.push_stopping_data(&state)?{
            Some(data_point) =>self.push_send_data(data_point).await?,
            None=>{}
        }
        Ok(())
    }
    async fn recceive_in_runnning(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        match  self.operating_states_chunk.push_running_data(&state)?{
            Some(data_point) =>self.push_send_data(data_point).await?,
            None=>{}
        }
        Ok(())
    }
    async fn recceive_to_stopping(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        self.last_machine_status = DemoCpb16Status::Stopping;

        let stopped_result = state.make_stopped_result()?;
        let mut send_result = Vec::<DataPoint>::new();
        send_result.push(stopped_result);
        self.sender.send(send_result).await?;

        // self.recceive_in_stopping(state).await?;

        Ok(())
    }
    async fn recceive_to_runnning(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        self.last_machine_status = DemoCpb16Status::Running;

        let worked_result = state.make_worked_result()?;
        let mut send_result = Vec::<DataPoint>::new();
        send_result.push(worked_result);
        self.sender.send(send_result).await?;

        self.chunk_first_production_count = 0;
        self.chunk_first_defect_count = 0;
        self.operating_states_chunk += 1;

        if self.operating_states_chunk

        // self.recceive_in_runnning(state).await?;さいしょ

        Ok(())
    }
    // set data
    // DemoCpb16ReceiveDataを消費する
    async fn set_operation_data(&mut self, data: DemoCpb16ReceiveData) -> anyhow::Result<()> {
        let new_dt = data.get_dt();
        let operation_point = data.parse_operation_data()?;
        self.operating_data.push(operation_point);
        self.last_operating_data_time = new_dt;

        if self.operating_data.len() >= self.send_chunk_size {
            self.send_operating_data().await?;
        }
        Ok(())
    }
    async fn set_sensor_data(&mut self, data: DemoCpb16ReceiveData) -> anyhow::Result<()> {
        let sensor_point = data.parse_sensor_data()?;
        self.sensor_data.push(sensor_point);

        if self.sensor_data.len() >= self.send_chunk_size {
            self.send_sensor_data().await?;
        }

        Ok(())
    }
    async fn set_operating_and_sensor(&mut self, data: DemoCpb16ReceiveData) -> anyhow::Result<()> {
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

    // // send data
    // // NOTE:Drop時に実行する
    // async fn send_operating_data(&mut self) -> anyhow::Result<()> {
    //     let send_data = std::mem::take(&mut self.operating_data);
    //     debug!("send_operating_data {} data", send_data.len());
    //     self.sender.send(send_data).await?;
    //     Ok(())
    // }
    // async fn send_sensor_data(&mut self) -> anyhow::Result<()> {
    //     let send_data = std::mem::take(&mut self.sensor_data);
    //     debug!("send_sensor_data {} data", send_data.len());
    //     self.sender.send(send_data).await?;

    //     Ok(())
    // }
}

impl Drop for DemoCpb16DataHundler {
    // NOTE:Dropトレイト内のエラー処理はどうする
    fn drop(&mut self) {
        // task::block_in_place(|| {
        //     let rt = tokio::runtime::Runtime::new().unwrap();
        //     rt.block_on(async {
        //         // 何か非同期的な処理を行う
        //         println!("data_manager drop start");
        //         if !self.operating_data.is_empty() {
        //             self.send_operating_data().await.unwrap();
        //         }
        //         if !self.sensor_data.is_empty() {
        //             self.send_sensor_data().await.unwrap();
        //         }
        //         println!("data_manager drop end");
        //     });
        // });
    }
}

pub struct DemoCpb16ReceiveData {
    dt: DateTime<Local>,
    data: String,
    status: DemoCpb16Status,
}
impl DemoCpb16ReceiveData {
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
        // DM0を稼働状況のデバイスとしている
        let status = match &data[0..5] {
            "00001" => DemoCpb16Status::Running,
            "00000" => DemoCpb16Status::Stopping,
            _ => DemoCpb16Status::Stopping,
        };
        // debug!("{}", &data[18..22]);
        Ok(Self { dt, data, status })
    }

    pub fn get_status(&self) -> DemoCpb16Status {
        self.status.clone()
    }
    pub fn get_dt(&self) -> DateTime<Local> {
        self.dt
    }
    pub fn get_data(&self) -> String {
        self.data.clone()
    }
}

struct DemoCpb16ReceiveState {
    receive_time: DateTime<Local>,
    status: DemoCpb16Status,
    working_id: u32,
    production_count: u32,
    defect_count: u32,
    last_production_count: u32,
    last_defect_count: u32,
    start_time: Option<DateTime<Local>>,
    last_start_time: DateTime<Local>,
    last_end_time: DateTime<Local>,
}
impl DemoCpb16ReceiveState {
    fn new(data: DemoCpb16ReceiveData) -> anyhow::Result<Self> {
        let res: Vec<&str> = data.data.split(' ').collect();
        if res.len() != 24 {
            anyhow::bail!("データ点数の異常")
        }

        // DM50 : 稼働のユニークIＤ
        let working_id: u32 = res[2].parse()?;
        // DM100 : 現在の稼働の生産数(袋)
        let production_count: u32 = res[3].parse()?;
        // DM102 : 現在の稼働の不良生産数(袋)
        let defect_count: u32 = res[4].parse()?;
        // DM104 : 前回稼働の生産数(袋)
        let last_production_count: u32 = res[5].parse()?;
        // DM106 : 前回稼働の不良生産数(袋)
        let last_defect_count: u32 = res[6].parse()?;

        let start_time = match data.get_status() {
            DemoCpb16Status::Running => {
                let dt = parse_datetime(res[7], res[8], res[9], res[10], res[11], res[12])?;
                Some(dt)
            }
            DemoCpb16Status::Stopping => None,
        };

        let last_start_time = parse_datetime(res[13], res[14], res[15], res[16], res[17], res[18])?;
        let last_end_time = parse_datetime(res[19], res[20], res[21], res[22], res[23], res[24])?;

        Ok(Self {
            receive_time: data.dt,
            status: data.status,
            working_id,
            production_count,
            defect_count,
            last_production_count,
            last_defect_count,
            start_time,
            last_start_time,
            last_end_time,
        })
    }

    fn make_worked_result(&self) -> anyhow::Result<DataPoint> {
        let time = match self.receive_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.receive_time.timestamp_nanos_opt()"),
        };

        let start_time = match self.last_start_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.last_start_time.timestamp_nanos_opt()"),
        };

        let end_time = match self.last_end_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.last_end_time.timestamp_nanos_opt()"),
        };

        let delta = (end_time - start_time) / 1_000_000_000;

        let worked_result = DataPoint::builder("demo_cpb16")
            .tag("info_type", "result")
            .field("is_working", true)
            .field("start_time", start_time)
            .field("end_time", end_time)
            .field("worked_second", delta)
            .field("production_count", self.last_production_count as i64)
            .field("defect_count", self.last_defect_count as i64)
            .timestamp(time)
            .build()?;

        Ok(worked_result)
    }

    fn make_stopped_result(&self) -> anyhow::Result<DataPoint> {
        let time = match self.receive_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.receive_time.timestamp_nanos_opt()"),
        };

        let start_time = match self.last_start_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.last_start_time.timestamp_nanos_opt()"),
        };

        let end_time = match self.last_end_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.last_end_time.timestamp_nanos_opt()"),
        };

        let delta = (end_time - start_time) / 1_000_000_000;

        let stopped_result = DataPoint::builder("demo_cpb16")
            .tag("info_type", "result")
            .field("is_working", false)
            .field("start_time", start_time)
            .field("end_time", end_time)
            .field("stoped_second", delta)
            .timestamp(time)
            .build()?;

        Ok(stopped_result)
    }
}

fn parse_datetime(
    year: &str,
    month: &str,
    day: &str,
    hour: &str,
    minute: &str,
    second: &str,
) -> anyhow::Result<DateTime<Local>> {
    let mut year: i32 = year.parse()?;
    year += 2000;
    let month: u32 = month.parse()?;
    let day: u32 = day.parse()?;
    let hour: u32 = hour.parse()?;
    let minute: u32 = minute.parse()?;
    let second: u32 = second.parse()?;
    let dt_result = Local.with_ymd_and_hms(year, month, day, hour, minute, second);
    let dt = match dt_result {
        chrono::LocalResult::Single(t) => t,
        _ => anyhow::bail!("時刻変換に失敗"),
    };
    Ok(dt)
}

#[derive(Debug, PartialEq, Clone)]
pub enum DemoCpb16Status {
    Running,
    Stopping,
}

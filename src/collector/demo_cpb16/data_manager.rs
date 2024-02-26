use chrono::{DateTime, Local, TimeZone};
use influxdb2::models::DataPoint;
use log::{debug, error};
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;

//
pub const SET_MONITER_COMMAND: &[u8] = b"\
    MWS DM0.U DM50.U DM100.U DM102.U DM104.U DM106.U \
    DM10.U DM12.U DM14.U DM16.U DM18.U DM20.U \
    DM22.U DM24.U DM26.U DM28.U DM30.U DM32.U \
    DM34.U DM36.U DM38.U DM40.U DM42.U DM44.U \
    DM2.U\r";

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
// DM2 : 過去データの有無  00000 : 無し、00001 : あり
const DATA_LENGTH: usize = 25;
const RESPONSE_LENGTH: usize = 149;
// 5×24+24-1=143この-1は何？
// 6×25-1=148

pub struct DemoCpb16DataManager {
    thread: Option<JoinHandle<DemoCpb16DataHandler>>,
    state: Option<DemoCpb16DataHandler>,
}
impl DemoCpb16DataManager {
    pub fn create(data_sender: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<Self> {
        let state: DemoCpb16DataHandler = DemoCpb16DataHandler::create(data_sender)?;

        Ok(Self {
            thread: None,
            state: Some(state),
        })
    }
    pub fn have_thread(&self) -> bool {
        self.thread.is_some()
    }
    pub async fn create_thread(
        &mut self,
        mut point_receiver: mpsc::Receiver<DemoCpb16ReceiveData>,
    ) -> anyhow::Result<()> {
        if self.have_thread() {
            anyhow::bail!("thread is already created")
        }
        let mut state = self.state.take().unwrap();
        let thread = tokio::spawn(async move {
            while let Some(data) = point_receiver.recv().await {
                match state.receive_response(data).await {
                    Ok(()) => {}
                    Err(r) => {
                        // TODO:ここのエラーハンドリングは用検討
                        error!("error in DemoCpb16DataManager::receive_response():{:?}", r)
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
// データ収集頻度以下で稼働⇒停止⇒稼働が発生すると問題かも

struct DemoCpb16OperationChunkData {
    operating_states_chunk_size: u32,
    operating_states_chunk_count: u32,
    chunk_last_production_count: u32,
    chunk_last_defect_count: u32,
    chunk_production: u32,
    chunk_defect: u32,
    chunk_work_second: i64,
}

impl DemoCpb16OperationChunkData {
    fn new() -> Self {
        Self {
            operating_states_chunk_size: 10,
            operating_states_chunk_count: 0,
            chunk_last_production_count: 0,
            chunk_last_defect_count: 0,
            chunk_production: 0,
            chunk_defect: 0,
            chunk_work_second: 0,
        }
    }

    fn push_running_data(
        &mut self,
        data: &DemoCpb16ReceiveState,
    ) -> anyhow::Result<Option<DataPoint>> {
        self.operating_states_chunk_count += 1;
        // self.chunk_production += data.production_count - self.chunk_last_production_count;
        let num = data.production_count - self.chunk_last_production_count;
        self.chunk_production += num;
        // self.chunk_defect += data.defect_count - self.chunk_last_production_count;
        // debug!(
        //     "data.defect_count:{:?},self.chunk_last_defect_count:{:?},",
        //     data.defect_count, self.chunk_last_defect_count
        // );
        let num = data.defect_count - self.chunk_last_defect_count;
        self.chunk_defect += num;
        self.chunk_last_production_count = data.production_count;
        self.chunk_last_defect_count = data.defect_count;
        self.chunk_work_second += 1;

        if self.operating_states_chunk_count == 10 {
            let data = self.make_working_data(data)?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    fn push_stopping_data(
        &mut self,
        data: &DemoCpb16ReceiveState,
    ) -> anyhow::Result<Option<DataPoint>> {
        self.operating_states_chunk_count += 1;
        self.chunk_work_second += 1;

        self.chunk_last_defect_count = 0;
        self.chunk_last_defect_count = 0;

        if self.operating_states_chunk_count == 10 {
            let data = self.make_working_data(data)?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    fn reset_chunk_from_data(&mut self, data: &DemoCpb16ReceiveState) {
        self.operating_states_chunk_count = 0;
        self.chunk_work_second = 0;
        self.chunk_defect = 0;
        self.chunk_production = 0;

        self.chunk_last_production_count = data.production_count;
        self.chunk_last_defect_count = data.defect_count;
    }

    fn make_working_data(&mut self, data: &DemoCpb16ReceiveState) -> anyhow::Result<DataPoint> {
        let is_working = match data.status {
            DemoCpb16Status::Running => true,
            DemoCpb16Status::Stopping => false,
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

        self.reset_chunk_from_data(data);

        Ok(working_data)
    }

    fn make_working_data_when_drop(self) -> anyhow::Result<DataPoint> {
        // let is_working = match self.chunk_last_production_count {
        //     0 => false,
        //     _ => true,
        // };
        let is_working = !matches!(self.chunk_last_production_count, 0);
        let time = match Local::now().timestamp_nanos_opt() {
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

        Ok(working_data)
    }
}
struct DemoCpb16DataHandler {
    sender: mpsc::Sender<Vec<DataPoint>>,
    last_machine_status: DemoCpb16Status,
    operating_states_chunk: DemoCpb16OperationChunkData,
    send_data_length: usize,
    operating_send_data: Vec<DataPoint>,
}

impl DemoCpb16DataHandler {
    fn create(sender: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<Self> {
        // TODO:定数はConfigに
        Ok(Self {
            sender,
            last_machine_status: DemoCpb16Status::Stopping,
            operating_states_chunk: DemoCpb16OperationChunkData::new(),
            send_data_length: 6,
            operating_send_data: Vec::<DataPoint>::new(),
        })
    }

    async fn receive_response(&mut self, data: DemoCpb16ReceiveData) -> anyhow::Result<()> {
        // debug!("receive_response");
        // 5秒毎にデータ収集してる
        let state = DemoCpb16ReceiveState::new(data)?;
        #[allow(unreachable_patterns)]
        match self.last_machine_status {
            DemoCpb16Status::Running => match state.status {
                DemoCpb16Status::Running => self.receive_in_running(state).await?,
                DemoCpb16Status::Stopping => self.receive_to_stopping(state).await?,
                _ => anyhow::bail!("受信データのMachineStatusが不正"),
            },
            DemoCpb16Status::Stopping => match state.status {
                DemoCpb16Status::Running => self.receive_to_running(state).await?,
                DemoCpb16Status::Stopping => self.receive_in_stopping(state).await?,
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
        if self.operating_send_data.len() == self.send_data_length {
            let send_data = std::mem::take(&mut self.operating_send_data);
            self.sender.send(send_data).await?;
        };
        Ok(())
    }

    async fn receive_in_stopping(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        if let Some(data_point) = self.operating_states_chunk.push_stopping_data(&state)? {
            self.push_send_data(data_point).await?
        }
        Ok(())
    }
    async fn receive_in_running(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        if let Some(data_point) = self.operating_states_chunk.push_running_data(&state)? {
            self.push_send_data(data_point).await?
        }
        Ok(())
    }
    async fn receive_to_stopping(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        self.last_machine_status = DemoCpb16Status::Stopping;

        // 稼働結果の送信
        // 停止時間は記録しない使用に
        // let stopped_result = state.make_stopped_result()?;

        // let send_result = vec![stopped_result; 1];
        // self.sender.send(send_result).await?;

        // オペレーション記録をチャンクにプッシュ
        self.receive_in_stopping(state).await?;

        Ok(())
    }
    async fn receive_to_running(&mut self, state: DemoCpb16ReceiveState) -> anyhow::Result<()> {
        self.last_machine_status = DemoCpb16Status::Running;
        // NOTE:モニタプログラム起動時かつPLCが稼働中の場合のハンドリングを追加
        if state.last_working_data.is_some() {
            // 稼働結果の送信
            let worked_result = state.make_worked_result()?;
            // let mut send_result = Vec::<DataPoint>::new();
            // send_result.push(worked_result);
            let send_result = vec![worked_result; 1];
            self.sender.send(send_result).await?;
        } else {
            debug!("receive_to_running:過去の稼働データがない")
        }

        // オペレーション記録をチャンクにプッシュ
        self.receive_in_running(state).await?;

        Ok(())
    }

    // NOTE:Drop時に実行する
    async fn send_chunk_data_when_drop(&mut self) -> anyhow::Result<()> {
        let send_data = std::mem::take(&mut self.operating_send_data);
        if !send_data.is_empty() {
            self.sender.send(send_data).await?;
        }

        Ok(())
    }
}

impl Drop for DemoCpb16DataHandler {
    // NOTE:Dropトレイト内のエラー処理はどうする
    fn drop(&mut self) {
        task::block_in_place(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // 何か非同期的な処理を行う
                println!("data_manager drop start");
                self.send_chunk_data_when_drop().await.unwrap();
                println!("data_manager drop end");
            });
        });
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
#[derive(Clone, Copy)]
struct LastWakingData {
    last_production_count: u32,
    last_defect_count: u32,
    last_start_time: DateTime<Local>,
    last_end_time: DateTime<Local>,
}
impl LastWakingData {
    // fn new(res: &Vec<&str>) -> anyhow::Result<Self> {
    fn new(res: &[&str]) -> anyhow::Result<Self> {
        // DM104 : 前回稼働の生産数(袋)
        let last_production_count: u32 = res[4].parse()?;
        // DM106 : 前回稼働の不良生産数(袋)
        let last_defect_count: u32 = res[5].parse()?;

        let last_start_time = parse_datetime(res[12], res[13], res[14], res[15], res[16], res[17])?;
        let last_end_time = parse_datetime(res[18], res[19], res[20], res[21], res[22], res[23])?;

        Ok(Self {
            last_production_count,
            last_defect_count,
            last_start_time,
            last_end_time,
        })
    }
}

struct DemoCpb16ReceiveState {
    receive_time: DateTime<Local>,
    status: DemoCpb16Status,
    working_id: u32,
    production_count: u32,
    defect_count: u32,
    start_time: Option<DateTime<Local>>,

    last_working_data: Option<LastWakingData>,
}
impl DemoCpb16ReceiveState {
    fn new(data: DemoCpb16ReceiveData) -> anyhow::Result<Self> {
        let res: Vec<&str> = data.data.split(' ').collect();
        if res.len() != DATA_LENGTH {
            anyhow::bail!("データ点数の異常")
        }

        // DM50 : 稼働のユニークIＤ
        let working_id: u32 = res[1].parse()?;
        // DM100 : 現在の稼働の生産数(袋)
        let production_count: u32 = res[2].parse()?;
        // DM102 : 現在の稼働の不良生産数(袋)
        let defect_count: u32 = res[3].parse()?;

        let start_time = match data.get_status() {
            DemoCpb16Status::Running => {
                let dt = parse_datetime(res[6], res[7], res[8], res[9], res[10], res[11])?;
                Some(dt)
            }
            DemoCpb16Status::Stopping => None,
        };

        let last_working_data = match res[24] {
            "00001" => Some(LastWakingData::new(&res)?),
            "00000" => None,
            _ => None,
        };

        Ok(Self {
            receive_time: data.dt,
            status: data.status,
            working_id,
            production_count,
            defect_count,
            start_time,
            last_working_data,
        })
    }

    fn make_worked_result(&self) -> anyhow::Result<DataPoint> {
        // データがない場合のエラーハンドリング
        let Some(data) = self.last_working_data else {
            anyhow::bail!("make_worked_result:データがないのに呼ばれている")
        };
        // NOTE: 初回起動時にPLCが稼働状態だった場合のハンドリング
        let time = match self.receive_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.receive_time.timestamp_nanos_opt()"),
        };

        let start_time = match data.last_start_time.timestamp_nanos_opt() {
            Some(t) => t,
            None => anyhow::bail!("in match self.last_start_time.timestamp_nanos_opt()"),
        };

        let end_time = match data.last_end_time.timestamp_nanos_opt() {
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
            .field("production_count", data.last_production_count as i64)
            .field("defect_count", data.last_defect_count as i64)
            .timestamp(time)
            .build()?;

        Ok(worked_result)
    }
    // 現状は不要なので実装しない
    // fn make_stopped_result(&self) -> anyhow::Result<DataPoint> {
    //     let time = match self.receive_time.timestamp_nanos_opt() {
    //         Some(t) => t,
    //         None => anyhow::bail!("in match self.receive_time.timestamp_nanos_opt()"),
    //     };

    //     let start_time = match self.last_start_time.timestamp_nanos_opt() {
    //         Some(t) => t,
    //         None => anyhow::bail!("in match self.last_start_time.timestamp_nanos_opt()"),
    //     };

    //     let end_time = match self.last_end_time.timestamp_nanos_opt() {
    //         Some(t) => t,
    //         None => anyhow::bail!("in match self.last_end_time.timestamp_nanos_opt()"),
    //     };

    //     let delta = (end_time - start_time) / 1_000_000_000;

    //     let stopped_result = DataPoint::builder("demo_cpb16")
    //         .tag("info_type", "result")
    //         .field("is_working", false)
    //         .field("start_time", start_time)
    //         .field("end_time", end_time)
    //         .field("stopped_second", delta)
    //         .timestamp(time)
    //         .build()?;

    //     Ok(stopped_result)
    // }
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
    // debug!(
    //     "year:{:?},month:{:?},,day:{:?},hour:{:?},minute:{:?},second:{:?}",
    //     year, month, day, hour, minute, second
    // );
    let dt_result = Local.with_ymd_and_hms(year, month, day, hour, minute, second);
    let dt = match dt_result {
        chrono::LocalResult::Single(t) => t,
        _ => {
            error!(
                "year:{:?},month:{:?},,day:{:?},hour:{:?},minute:{:?},second:{:?}",
                year, month, day, hour, minute, second
            );
            anyhow::bail!("時刻変換に失敗")
        }
    };
    Ok(dt)
}

#[derive(Debug, PartialEq, Clone)]
pub enum DemoCpb16Status {
    Running,
    Stopping,
}

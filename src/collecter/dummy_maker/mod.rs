#![allow(dead_code)]

use chrono::Local;
use influxdb2::models::DataPoint;
use log::{debug, warn};
use tokio::sync::mpsc;
use tokio::task;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use rand::Rng;
use rand::SeedableRng;

pub struct DummyDataMaker {
    sender: mpsc::Sender<Vec<DataPoint>>,
    thread: Option<GenerateThread>,
}
impl DummyDataMaker {
    pub fn new() -> anyhow::Result<(Self, mpsc::Receiver<Vec<DataPoint>>)> {
        let (tx, rx) = mpsc::channel(32);

        Ok((
            Self {
                sender: tx,
                thread: None,
            },
            rx,
        ))
    }
    // 他のメソッドと合わせるために非同期関数にしている
    pub async fn start_making_data(&mut self) -> anyhow::Result<()> {
        if self.thread.is_some() {
            warn!("already making data in DummyDataMaker::start_making_data");
            anyhow::bail!("already making data in DummyDataMaker::start_making_data")
        }

        // 処理
        let sender = self.sender.clone();
        let generate_thread = GenerateThread::start(sender)?;

        self.thread = Some(generate_thread);
        debug!("DummyDataMaker start making data");
        Ok(())
    }

    pub async fn stop_making_data(&mut self) -> anyhow::Result<()> {
        if let Some(thread) = self.thread.take() {
            thread.stop().await?;
        } else {
            anyhow::bail!("not making data in DummyDataMaker::stop_making_data")
        }

        debug!("DummyDataMaker stop");
        Ok(())
    }
}
impl Drop for DummyDataMaker {
    fn drop(&mut self) {
        task::block_in_place(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if self.thread.is_some() {
                    self.stop_making_data().await.unwrap();
                }
            });
        });
    }
}

struct GenerateThread {
    point_generate_thread: JoinHandle<()>,
    point_manage_thread: JoinHandle<()>,
    stop_sender: mpsc::Sender<()>,
}

impl GenerateThread {
    fn start(tx: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<Self> {
        let (stop_sender, stop_receiver) = mpsc::channel(32);
        let (point_sender, point_receiver) = mpsc::channel(32);
        let point_generate_thread = tokio::spawn(async move {
            // データ変換スレッドを作成する
            let _ = generate_data_point(point_sender, stop_receiver).await;
        });
        let point_manage_thread = tokio::spawn(async move {
            // Vec<DataPoint>に変換するスレッド
            let _ = collect_points_to_vec(tx, point_receiver).await;
        });
        Ok(Self {
            point_generate_thread,
            point_manage_thread,
            stop_sender,
        })
    }

    async fn stop(self) -> anyhow::Result<()> {
        self.stop_sender.send(()).await?;
        // 完了を待つ処理
        self.point_generate_thread.await?;
        self.point_manage_thread.await?;
        Ok(())
    }
}
async fn collect_points_to_vec(
    tx: mpsc::Sender<Vec<DataPoint>>,
    mut point_receiver: mpsc::Receiver<DataPoint>,
) -> anyhow::Result<()> {
    let mut points: Vec<DataPoint> = Vec::<DataPoint>::new();
    while let Some(data) = point_receiver.recv().await {
        points.push(data);
        if points.len() >= 50 {
            tx.send(points).await?;
            points = Vec::<DataPoint>::new();
        }
    }

    if !points.is_empty() {
        tx.send(points).await?;
    }

    Ok(())
}

// データ生成スレッドの作成　50msecでデータを送信するスレッド
async fn generate_data_point(
    tx: mpsc::Sender<DataPoint>,
    mut stop_receiver: mpsc::Receiver<()>,
) -> anyhow::Result<()> {
    let mut field1 = 50.0;
    let mut field2 = 50.0;
    let mut field3 = 50.0;
    let mut next_loop_start_time = Instant::now();
    // let mut rng = rand::thread_rng();
    let mut rng = rand::rngs::StdRng::from_entropy();
    // Sendを実装している乱数生成器
    // use rand::SeedableRng;で使える

    loop {
        next_loop_start_time += Duration::from_millis(50);
        let now = Instant::now();

        tokio::select! {
            _ = stop_receiver.recv() => {
                // ループするのでOneshotレシーバーをル買えない
                break;
            }
            _ = tokio::time::sleep(next_loop_start_time - now) =>{
                let time = Local::now().timestamp_nanos_opt().unwrap();
                field1 += rng.gen_range(-100..=100) as f64 / 10.0;
                field2 += rng.gen_range(-100..=100) as f64 / 10.0;
                field3 += rng.gen_range(-100..=100) as f64 / 10.0;

                let point = match generate_tempurature_data_point(field1, field2, field3, time){
                    Ok(point) => point,
                    Err(e) => {
                        return Err(e);
                    }
                };
                if let Err(e) = tx.send(point).await {
                    // エラーハンドリング
                    return Err(e.into());
                }
            }
        }
    }

    Ok(())
}

fn generate_tempurature_data_point(
    tempureture_1: f64,
    tempureture_2: f64,
    tempureture_3: f64,
    time: i64,
) -> anyhow::Result<DataPoint> {
    let point = DataPoint::builder("machine_1")
        .tag("sensor_type", "tempurature")
        .field("tempureture_1", tempureture_1)
        .field("tempureture_2", tempureture_2)
        .field("tempureture_3", tempureture_3)
        .timestamp(time)
        .build()?;
    Ok(point)
}

use chrono::{DateTime, Local};
use futures::stream;
use influxdb2::models::DataPoint;
use influxdb2::Client;
use log::{debug, error, warn};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

use rand::Rng;
pub struct InfluxDB {
    client: Client,
    bucket: String,
}

impl InfluxDB {
    pub fn create_from_env() -> anyhow::Result<Self> {
        let host = std::env::var("INFLUXDB_HOST")?;
        debug!("host:{}", host);
        let org = std::env::var("INFLUXDB_ORG")?;
        let token = std::env::var("INFLUXDB_TOKEN")?;
        let bucket = std::env::var("INFLUXDB_BUCKET")?;
        let client = Client::new(host, org, token);

        Ok(Self { client, bucket })
    }
    pub async fn start_send_data(
        self,
        mut rx: mpsc::Receiver<Vec<DataPoint>>,
    ) -> anyhow::Result<()> {
        debug!("start_send_data");
        while let Some(points) = rx.recv().await {
            debug!("receive {:?} data", points.len());

            let result = self.client.write(&self.bucket, stream::iter(points)).await;

            match result {
                Ok(()) => {
                    debug!("送信完了")
                }
                Err(r) => {
                    error!("{:?}", r)
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
enum MakerState {
    Stopping,
    Working,
}

pub struct DummyDataMaker {
    sender: mpsc::Sender<Vec<DataPoint>>,
    manager_hundle: Option<JoinHandle<()>>,
    state: MakerState,
}
impl DummyDataMaker {
    pub fn new() -> anyhow::Result<(Self, mpsc::Receiver<Vec<DataPoint>>)> {
        let (tx, rx) = mpsc::channel(32);

        Ok((
            Self {
                sender: tx,
                manager_hundle: None,
                state: MakerState::Stopping,
            },
            rx,
        ))
    }
    pub async fn start_making_data(&mut self) -> anyhow::Result<()> {
        if self.state != MakerState::Stopping {
            warn!(
                "start_data_collection can not execute: state = {:?}",
                self.state
            );
            anyhow::bail!(
                "start_data_collection can not execute: state = {:?}",
                self.state
            )
        }
        // 処理
        let sender = self.sender.clone();
        let manager_hundle = tokio::spawn(async move {
            // データ変換スレッドを作成する
            generate_data(sender).await;
        });

        self.state = MakerState::Working;
        self.manager_hundle = Some(manager_hundle);
        debug!("DummyDataMaker is Working");
        Ok(())
    }

    pub fn stop_data_collection(&mut self) -> anyhow::Result<()> {
        if self.state != MakerState::Working {
            warn!(
                "stop_data_collection can not execute: state = {:?}",
                self.state
            );
            anyhow::bail!(
                "stop_data_collection can not execute: state = {:?}",
                self.state
            )
        }
        self.state = MakerState::Stopping;
        // self.interface_hundle = None;
        self.manager_hundle = None;
        debug!("DummyDataMaker is Stopping");

        Ok(())
    }
}

async fn generate_data(tx: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<()> {
    // データ生成処理
    // 50msごとにデータを生成してチャンネルに送信
    // 200データ⇒10s毎にtxに送信×60⇒10分分のデータ
    let mut field1 = 50.0;
    let mut field2 = 50.0;
    let mut field3 = 50.0;
    let mut next_loop_start_time = Instant::now();

    // for _ in 0..60 {
    loop {
        let mut points: Vec<DataPoint> = Vec::<DataPoint>::new();
        for _ in 0..200 {
            // println!("{},{},{}", field1, field2, field3);
            next_loop_start_time += Duration::from_millis(50);
            let time = Local::now().timestamp_nanos_opt().unwrap();

            let point = generate_tempurature_data_point(field1, field2, field3, time)?;
            points.push(point);

            {
                let mut rng = rand::thread_rng();
                field1 += rng.gen_range(-100..=100) as f64 / 10.0;
                field2 += rng.gen_range(-100..=100) as f64 / 10.0;
                field3 += rng.gen_range(-100..=100) as f64 / 10.0;
            }

            let now = Instant::now();
            if next_loop_start_time > now {
                tokio::time::sleep(next_loop_start_time - now).await;
            }
        }

        tx.send(points).await?;
        debug!("send data in generate_data");
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

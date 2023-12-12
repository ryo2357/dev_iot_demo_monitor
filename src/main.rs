use chrono::{DateTime, Local};
use futures::stream;
use influxdb2::models::DataPoint;
use influxdb2::Client;
use log::debug;
use rand::Rng;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, MissedTickBehavior};

mod interface;
use interface::DemoMachineStatus;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    data_collect_test().await?;
    Ok(())
}

#[allow(dead_code)]
async fn data_collect_test() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    mylogger::init();
    let (mut machine_status, _) = DemoMachineStatus::create_from_env()?;
    machine_status.monitor_test().await?;
    Ok(())
}

#[allow(dead_code)]
async fn interface_check() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    mylogger::init();
    let (mut machine_status, _) = DemoMachineStatus::create_from_env()?;
    machine_status.check_connection().await?;
    Ok(())
}

struct GenerateThread {
    runner: JoinHandle<anyhow::Result<()>>,
}
impl GenerateThread {
    pub fn get_runner(self) -> JoinHandle<anyhow::Result<()>> {
        self.runner
    }
}

#[allow(dead_code)]
async fn main_thread() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let (tx, rx) = mpsc::channel(32);

    // データ生成タスクをスポーン
    let generate_task = tokio::spawn(generate_data(tx));
    let task = GenerateThread {
        runner: generate_task,
    };

    // データ送信タスクをスポーン
    let send_task = tokio::spawn(send_data(rx));

    // 両方のタスクが終了するまで待機
    // let _ = tokio::join!(generate_task, send_task);
    let _ = tokio::join!(task.get_runner(), send_task);

    // 他の処理
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

async fn generate_data(tx: mpsc::Sender<Vec<DataPoint>>) -> anyhow::Result<()> {
    // データ生成処理
    // 50msごとにデータを生成してチャンネルに送信
    // 200データ⇒10s毎にtxに送信×60⇒10分分のデータ
    let mut field1 = 50.0;
    let mut field2 = 50.0;
    let mut field3 = 50.0;
    let mut next_loop_start_time = Instant::now();

    for _ in 0..20 {
        // loop {
        let mut points: Vec<DataPoint> = Vec::<DataPoint>::new();
        for _ in 0..10 {
            // println!("{},{},{}", field1, field2, field3);
            next_loop_start_time += Duration::from_millis(500);
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
    }

    Ok(())
}

async fn send_data(mut rx: mpsc::Receiver<Vec<DataPoint>>) -> anyhow::Result<()> {
    // クライアント設定
    let host = std::env::var("INFLUXDB_HOST").unwrap();
    let org = std::env::var("INFLUXDB_ORG").unwrap();
    let token = std::env::var("INFLUXDB_TOKEN").unwrap();
    let bucket = std::env::var("INFLUXDB_BUCKET").unwrap();

    let client = Client::new(host, org, token);

    // データ送信処理

    while let Some(points) = rx.recv().await {
        let dt: DateTime<Local> = Local::now();

        println!("{:?}:receive {:?} data", dt, points.len());

        let result = client.write(&bucket, stream::iter(points)).await;

        match result {
            Ok(()) => {}
            Err(r) => {
                println!("{:?}", r)
            }
        }
    }

    Ok(())
}

use tokio::time::Duration;

mod collecter;
mod influxdb;
mod verify_influxdb;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    mylogger::init();
    verify_influxdb().await?;
    Ok(())
}

#[allow(dead_code)]
async fn verify_influxdb() -> anyhow::Result<()> {
    let (mut collecter, receiver) = verify_influxdb::DummyDataMaker::new()?;
    let data_base = verify_influxdb::InfluxDB::create_from_env()?;

    tokio::spawn(data_base.start_send_data(receiver));

    // 10分間データ収集を実行
    collecter.start_making_data().await?;
    wait(12).await;
    log::info!("12秒間データを作成完了");

    collecter.stop_making_data().await?;
    wait(50).await;
    log::info!("50秒停止完了");
    collecter.start_making_data().await?;
    wait(22).await;
    log::info!("22秒間データを作成完了");
    collecter.stop_making_data().await?;

    log::info!("finish");
    // TODO: collecterをstop後。送信完了まで待つ必要がある

    Ok(())
}

#[allow(dead_code)]
async fn test_run() -> anyhow::Result<()> {
    let (mut collecter, receiver) = collecter::DemoMachineCollecter::create_from_env().await?;
    tokio::spawn(influxdb::send_data(receiver));

    // 10分間データ収集を実行
    log::info!("before start");
    collecter.start_data_collection().await?;
    log::info!("after start");
    wait(600).await;
    log::info!("after wait");
    collecter.stop_data_collection()?;

    Ok(())
}

#[allow(dead_code)]
async fn endless_run() -> anyhow::Result<()> {
    let (mut collecter, receiver) = collecter::DemoMachineCollecter::create_from_env().await?;
    let send_task = tokio::spawn(influxdb::send_data(receiver));

    collecter.start_data_collection().await?;
    let _ = tokio::join!(send_task);
    Ok(())
}

async fn wait(sec: u64) {
    tokio::time::sleep(Duration::from_secs(sec)).await;
}

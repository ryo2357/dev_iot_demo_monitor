use tokio::time::Duration;

mod collecter;
mod influxdb;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    mylogger::init();
    test_run().await?;
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

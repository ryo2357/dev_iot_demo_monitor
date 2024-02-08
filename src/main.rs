use tokio::sync::mpsc;
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
async fn verify_influxdb() -> anyhow::Result<()> {
    let (mut collecter, receiver) = collecter::dummy_maker::DummyDataMaker::new()?;
    let mut data_base = influxdb::InfluxDB::create_from_env()?;

    data_base.start_send_data(receiver).await?;

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

    // sender をドロップするためにcollecterをドロップする必要がある
    // ドロップするしないとwait_thread_finishedで無限に待つことになる
    drop(collecter);
    data_base.wait_thread_finished().await?;
    log::info!("finish");

    Ok(())
}

#[allow(dead_code)]
async fn test_run() -> anyhow::Result<()> {
    let (data_sender, data_receiver) = mpsc::channel(32);
    let mut collecter =
        collecter::demo_mashine::DemoMachineCollecter::create_from_env(data_sender).await?;
    let mut data_base = influxdb::InfluxDB::create_from_env()?;

    data_base.start_send_data(data_receiver).await?;

    // 10分間データ収集を実行
    collecter.start_data_collection().await?;
    log::info!("150秒間データ収集を開始");
    wait(150).await;

    // 10分間データ収集を停止
    collecter.stop_data_collection().await?;
    log::info!("150秒間データ収集を停止");
    wait(150).await;

    // 10分間データ収集を実行
    collecter.start_data_collection().await?;
    log::info!("150秒間データ収集を開始");
    wait(150).await;

    collecter.stop_data_collection().await?;
    log::info!("データ収集を終了");

    Ok(())
}

#[allow(dead_code)]
async fn endless_run() -> anyhow::Result<()> {
    // let (mut collecter, receiver) = collecter::DemoMachineCollecter::create_from_env().await?;
    // let send_task = tokio::spawn(influxdb::send_data(receiver));

    // collecter.start_data_collection().await?;
    // let _ = tokio::join!(send_task);
    Ok(())
}

async fn wait(sec: u64) {
    tokio::time::sleep(Duration::from_secs(sec)).await;
}

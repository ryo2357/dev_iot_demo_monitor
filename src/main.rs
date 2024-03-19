#[allow(unused_imports)]
use log::{debug, error};
use tokio::sync::mpsc;
use tokio::time::Duration;

mod collector;
mod influxdb;
mod runner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mylogger::init();
    dotenv::dotenv().ok();
    if let Err(r) = demo_cpb16_running().await {
        error!("{{:?}}:{:?}", r);
        anyhow::bail!("error at demo_cpb16_running")
    };
    Ok(())
}

#[allow(dead_code)]
async fn demo_cpb16_set_time() -> anyhow::Result<()> {
    let mut debugger = collector::demo_cpb16::DemoCpb16Debugger::create_from_env().await?;
    log::info!("コマンドテスト開始");
    // debugger.set_time_dummy().await?;
    debugger.set_time().await?;
    log::info!("コマンドテスト完了");
    Ok(())
}

#[allow(dead_code)]
async fn demo_cpb16_running() -> anyhow::Result<()> {
    log::info!("製袋16号機のデモデータ収集開始");
    let mut runner = runner::demo_bench_run::Runner::create_from_env().await?;
    runner.execute().await?;
    Ok(())
}

#[allow(dead_code)]
async fn demo_cpb16_debug_check() -> anyhow::Result<()> {
    let mut debugger = collector::demo_cpb16::DemoCpb16Debugger::create_from_env().await?;
    debugger.start_debug_monitor().await?;
    wait(12).await;
    log::info!("12秒間のモニター完了");
    debugger.stop_debug_monitor().await?;
    Ok(())
}

#[allow(dead_code)]
async fn verify_influxdb() -> anyhow::Result<()> {
    let (mut collector, receiver) = collector::dummy_maker::DummyDataMaker::new()?;
    let mut data_base = influxdb::InfluxDB::create_from_env()?;

    data_base.start_send_data(receiver).await?;

    // 10分間データ収集を実行
    collector.start_making_data().await?;
    wait(12).await;
    log::info!("12秒間データを作成完了");

    collector.stop_making_data().await?;
    wait(50).await;
    log::info!("50秒停止完了");
    collector.start_making_data().await?;
    wait(22).await;
    log::info!("22秒間データを作成完了");
    collector.stop_making_data().await?;

    // sender をドロップするためにcollectorをドロップする必要がある
    // ドロップするしないとwait_thread_finishedで無限に待つことになる
    drop(collector);
    data_base.wait_thread_finished().await?;
    log::info!("finish");

    Ok(())
}

#[allow(dead_code)]
async fn test_run() -> anyhow::Result<()> {
    let (data_sender, data_receiver) = mpsc::channel(32);
    let mut collector =
        collector::demo_machine::DemoMachineCollector::create_from_env(data_sender).await?;
    let mut data_base = influxdb::InfluxDB::create_from_env()?;

    data_base.start_send_data(data_receiver).await?;

    // 10分間データ収集を実行
    collector.start_data_collection().await?;
    log::info!("150秒間データ収集を開始");
    wait(150).await;

    // 10分間データ収集を停止
    collector.stop_data_collection().await?;
    log::info!("150秒間データ収集を停止");
    wait(150).await;

    // 10分間データ収集を実行
    collector.start_data_collection().await?;
    log::info!("150秒間データ収集を開始");
    wait(150).await;

    collector.stop_data_collection().await?;
    log::info!("データ収集を終了");

    Ok(())
}

#[allow(dead_code)]
async fn test_run_2() -> anyhow::Result<()> {
    let (data_sender, data_receiver) = mpsc::channel(32);
    let mut collector =
        collector::demo_machine::DemoMachineCollector::create_from_env(data_sender).await?;
    let mut data_base = influxdb::InfluxDB::create_from_env()?;

    data_base.start_send_data(data_receiver).await?;

    // 10分間データ収集を実行
    collector.start_data_collection().await?;
    log::info!("150秒間データ収集を開始");
    wait(150).await;

    // 10分間データ収集を停止
    collector.stop_data_collection().await?;
    log::info!("150秒間データ収集を停止");
    wait(150).await;

    // 10分間データ収集を実行
    collector.start_data_collection().await?;
    log::info!("150秒間データ収集を開始");
    wait(150).await;

    collector.stop_data_collection().await?;
    log::info!("データ収集を終了");

    Ok(())
}

#[allow(dead_code)]
async fn endless_run() -> anyhow::Result<()> {
    // let (mut collector, receiver) = collector::DemoMachinecollector::create_from_env().await?;
    // let send_task = tokio::spawn(influxdb::send_data(receiver));

    // collector.start_data_collection().await?;
    // let _ = tokio::join!(send_task);
    Ok(())
}

async fn wait(sec: u64) {
    tokio::time::sleep(Duration::from_secs(sec)).await;
}

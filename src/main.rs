mod collecter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    test_run().await?;
    Ok(())
}

async fn test_run() -> anyhow::Result<()> {
    let (mut collecter, mut receiver) = collecter::DemoMachineCollecter::create_from_env().await?;

    Ok(())
}

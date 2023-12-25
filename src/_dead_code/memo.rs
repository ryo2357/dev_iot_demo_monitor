pub struct DemoMachineCollecter {
    converter_hundle: Option<JoinHandle<()>>,
}

impl DemoMachineCollecter {
    pub async fn start_data_collection(&mut self) -> anyhow::Result<()> {
        let conver_hundle = tokio::spawn(async move {
            // データ変換スレッドを作成する
        });
        self.converter_hundle = Some(conver_hundle);

        Ok(())
    }

    pub async fn stop_data_collection(&mut self) -> anyhow::Result<()> {
        self.converter_hundle = None;

        Ok(())
    }
}

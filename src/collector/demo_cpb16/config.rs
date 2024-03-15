pub use super::data_manager::SET_MONITOR_COMMAND;

// 機械稼働時は1000msec間隔
pub const MONITOR_INTERVAL: u64 = 1000;
// 機械停止時時は1秒間隔
pub const INTERVAL_WHEN_MACHINE_STOP: u64 = 1_000;

// const MONITOR_INTERVAL: u64 = 50;
// const INTERVAL_WHEN_MACHINE_STOP: u64 = 1000;

pub const CHECK_COMMAND: &[u8] = b"?K\r";
pub const CHECK_RESPONSE: &str = "55";
pub const OK_RESPONSE: &str = "OK";
pub const COMMAND_ABNORMAL_RESPONSE: &str = "E1";
pub const MONITOR_READOUT_COMMAND: &[u8] = b"MWR\r";
pub const TIME_PREFERENCE_COMMAND: &[u8] = b"WRT ";

#[derive(Clone)]
pub struct DemoCpb16Config {
    address: String,
}
impl DemoCpb16Config {
    pub fn create_from_env() -> anyhow::Result<Self> {
        let address = std::env::var("DemoCpb16StatusConfigAddress")?;
        Ok(Self { address })
    }
    pub fn get_address(&self) -> String {
        self.address.to_owned()
    }

    // 現在時刻を送信、テスト検証ではハードコーディングで試す
    pub fn get_time_preference_command(&self) -> Vec<u8> {
        let mut command = TIME_PREFERENCE_COMMAND.to_owned();
        command.extend(b"24 03 15 10 20 30 5\r");
        command
    }
}

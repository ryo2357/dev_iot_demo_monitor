use super::data_manager::SET_MONITER_COMMAND;

// 機械稼働時は50msec間隔
const MONITOR_INTERVAL: u64 = 50;
// 機械停止時時は1000msec間隔
const INTERVAL_WHEN_MACHINE_STOP: u64 = 5000;
const CHECK_COMMAND: &[u8] = b"?K\r";
const CHECK_RESPONSE: &str = "55";
const MONITOR_READOUT_COMMAND: &[u8] = b"MWR\r";

#[derive(Clone)]
pub struct DemoMachineConfig {
    address: String,
    check_command: Vec<u8>,
    check_response: String,
    set_moniter_command: Vec<u8>,
    monitor_readout_command: Vec<u8>,
    monitor_interval: u64,
    interval_when_machine_stop: u64,
}
impl DemoMachineConfig {
    pub fn create_from_env() -> anyhow::Result<Self> {
        let address = std::env::var("DemoMachineStatusConfigAddress")?;

        let check_command: Vec<u8> = CHECK_COMMAND.into();
        let check_response = CHECK_RESPONSE.into();

        let set_moniter_command: Vec<u8> = SET_MONITER_COMMAND.into();
        let monitor_readout_command: Vec<u8> = MONITOR_READOUT_COMMAND.into();

        let monitor_interval = MONITOR_INTERVAL;
        let interval_when_machine_stop = INTERVAL_WHEN_MACHINE_STOP;

        Ok(Self {
            address,
            check_command,
            check_response,
            set_moniter_command,
            monitor_readout_command,
            monitor_interval,
            interval_when_machine_stop,
        })
    }
    pub fn get_address(&self) -> String {
        self.address.to_owned()
    }

    pub fn get_check_command(&self) -> Vec<u8> {
        self.check_command.to_owned()
    }

    pub fn get_check_response(&self) -> String {
        self.check_response.to_owned()
    }
    pub fn get_set_moniter_command(&self) -> Vec<u8> {
        self.set_moniter_command.to_owned()
    }
    pub fn get_monitor_readout_command(&self) -> Vec<u8> {
        self.monitor_readout_command.to_owned()
    }
    pub fn get_monitor_interval(&self) -> u64 {
        self.monitor_interval.to_owned()
    }
    pub fn get_interval_when_machine_stop(&self) -> u64 {
        self.interval_when_machine_stop.to_owned()
    }
}

use super::data_manager::SET_MONITER_COMMAND;

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

        let check_command: Vec<u8> = "?K\r".into();
        let check_response = "55".into();

        let set_moniter_command: Vec<u8> = SET_MONITER_COMMAND.into();
        let monitor_readout_command: Vec<u8> = "MWR\r".into();

        // 機械稼働時は50msec間隔
        let monitor_interval = 50;
        // 機械停止時時は5000msec間隔
        let interval_when_machine_stop = 5000;

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

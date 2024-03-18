use chrono::Local;
#[allow(unused_imports)]
use log::{debug, error};

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

    pub fn get_time_preference_command(&self) -> Vec<u8> {
        let command = TIME_PREFERENCE_COMMAND.to_owned();
        let command = std::str::from_utf8(&command).unwrap();

        let now_string = Local::now().to_rfc2822();
        let now_split: Vec<&str> = now_string.split(' ').collect();
        // Tue, 1 Jul 2003 10:52:37 +0200
        let week: &str = match now_split[0] {
            "Sun," => "0",
            "Mon," => "1",
            "Tue," => "2",
            "Wed," => "3",
            "Thu," => "4",
            "Fri," => "5",
            "Sat," => "6",
            _ => "7",
        };

        // debug!("week:{:?},{:?}", week, now_split[0]);

        let day: u16 = now_split[1].parse().unwrap();
        let day: String = format!("{:0>2}", day);

        // debug!("day:{:?},{:?}", day, now_split[1]);

        let month = match now_split[2] {
            "Jan" => "01",
            "Feb" => "02",
            "Mar" => "03",
            "Apr" => "04",
            "May" => "05",
            "Jun" => "06",
            "Jul" => "07",
            "Aug" => "08",
            "Sep" => "09",
            "Oct" => "10",
            "Nov" => "11",
            "Dec" => "12",
            _ => "00",
        };

        // debug!("month:{:?},{:?}", month, now_split[2]);

        let time: String = now_split[4].replace(':', " ");
        // debug!("time:{:?},{:?}", time, now_split[4]);

        let year: u32 = now_split[3].parse().unwrap();
        let year = year - 2000;
        let year: String = format!("{:0>2}", year);

        // debug!("year:{:?},{:?}", year, now_split[3]);

        let command: String = command.to_string()
            + &year
            + " "
            + month
            + " "
            + &day
            + " "
            + &time
            + " "
            + week
            + "\r";
        // debug!("command:{:?}", command);

        // let mut vec = TIME_PREFERENCE_COMMAND.to_owned();
        // vec.extend(b"24 03 15 10 20 30 5\r");
        command.into_bytes()
    }

    pub fn get_time_preference_dummy_command(&self) -> Vec<u8> {
        let mut vec = TIME_PREFERENCE_COMMAND.to_owned();
        vec.extend(b"24 03 15 10 20 30 5\r");
        vec
    }
}

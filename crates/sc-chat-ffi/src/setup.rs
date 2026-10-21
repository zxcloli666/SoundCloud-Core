#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChatSetup {
    pub data_dir: String,
    pub platform: String,
    pub device_name: String,
    pub app_version: String,
}

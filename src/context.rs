use crate::infrastructure::config::AppConfig;

#[derive(Debug)]
pub struct AppContext {
    pub config: AppConfig,
}

impl AppContext {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }
}

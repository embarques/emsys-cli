use tracing_subscriber::EnvFilter;

use crate::{context::AppContext, infrastructure::config::AppConfig};

pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

pub fn build_context() -> anyhow::Result<AppContext> {
    let config = AppConfig::from_env()?;
    Ok(AppContext::new(config))
}

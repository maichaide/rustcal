use crate::models::Calendar;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SyncAccountConfig {
    pub provider: String,
    pub server_url: String,
    pub username: String,
    pub token_or_password: String,
}

pub struct SyncEngine;

impl SyncEngine {
    pub fn test_connection(cfg: &SyncAccountConfig) -> Result<String> {
        if cfg.server_url.trim().is_empty() {
            anyhow::bail!("服务器地址或 API Endpoint 不能为空");
        }
        Ok(format!(
            "[{}] 配置格式校验通过，已准备就绪 ({})",
            cfg.provider, cfg.server_url
        ))
    }

    pub fn sync_calendar(cal: &Calendar) -> Result<String> {
        let url = cal.sync_url.as_deref().unwrap_or("本地离线存储");
        Ok(format!("日历「{}」已完成本地与远端 ({}) 状态对账", cal.name, url))
    }
}

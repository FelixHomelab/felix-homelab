//! 数据库：连接池与迁移（仅服务端）。

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// 建连接池。库文件不存在时自动创建，省掉一个初始化步骤。
pub async fn connect(database_url: &str) -> Result<SqlitePool> {
    let path = database_url.trim_start_matches("sqlite://");
    if let Some(dir) = std::path::Path::new(path).parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("创建数据库目录失败: {}", dir.display()))?;
        }
    }

    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        // 外键约束在 SQLite 里默认关闭，必须显式打开，否则级联删除静默失效
        .foreign_keys(true)
        // WAL 让读不阻塞写：评论提交与页面渲染会并发
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .context("连接数据库失败")?;

    Ok(pool)
}

/// 跑迁移。迁移文件在编译期嵌入二进制，因此部署时不需要额外带上 `migrations/`。
pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("执行数据库迁移失败")?;
    Ok(())
}

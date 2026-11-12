//! 主节点单元测试共享夹具。
//!
//! 提供内存数据库、主节点账号与数据面状态，避免 `api` / `service` /
//! `web_rpc` 三个测试模块各自重复搭建环境。所有数据都落在 `:memory:`，
//! 测试之间互不影响。

use std::sync::Arc;

use clap::Parser;
use rust_vless_common::UserRegistry;
use rust_vless_dataplane::{DataPlaneConfig, DataPlaneState};

use crate::config::MasterConfig;
use crate::db::{AdminRecord, Db, Role};
use crate::state::AppState;

/// 测试管理员邮箱，与 [`ADMIN_PASSWORD`] 配套使用。
pub const ADMIN_EMAIL: &str = "admin@example.com";
/// 测试管理员密码；仅在内存数据库中有效。
pub const ADMIN_PASSWORD: &str = "password123";

/// 构建一份可直接用于服务层与 RPC 测试的应用状态。
pub async fn app_state() -> Arc<AppState> {
    let db = Db::open(":memory:").await.unwrap();
    let hash = crate::auth::hash_password(ADMIN_PASSWORD).unwrap();
    db.create_admin(ADMIN_EMAIL, &hash, Role::Admin)
        .await
        .unwrap();
    let config = MasterConfig::parse_from(["rust-vless-master"]);
    let master_id = uuid::Uuid::new_v4();
    db.ensure_master_node(
        master_id,
        "master",
        "http://127.0.0.1:8080",
        "http://127.0.0.1:50051",
    )
    .await
    .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let plane = DataPlaneState::new(
        master_id,
        "master".into(),
        Arc::new(UserRegistry::new()),
        16,
        tx,
        DataPlaneConfig {
            max_hops: 8,
            allow_direct_fallback: true,
        },
    );
    let state = AppState::new(db, config, master_id, plane);
    state.refresh_dataplane().await.unwrap();
    state
}

/// 读取测试管理员记录（含 password hash 与角色）。
pub async fn admin_record(state: &AppState) -> AdminRecord {
    state
        .db
        .find_admin_by_email(ADMIN_EMAIL)
        .await
        .unwrap()
        .unwrap()
}

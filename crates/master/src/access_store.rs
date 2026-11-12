//! 访问日志的独立存储与查询。
//!
//! 访问日志不再写入主数据库，而是按 JSON Lines 追加到单独文件：
//! - 写入是顺序追加，成本低，也不会和业务表争抢 SQLite 锁；
//! - 查询时把文件读入 DuckDB 内存表执行过滤、分页与聚合。
//!   过滤在流式读取阶段先应用，因此带条件的分页只把匹配行放进内存表。
//!
//! 文件保留策略由调用方按天触发，`prune` 会重写文件只保留窗口内的记录。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::db::{AccessLogInput, AccessLogPage, AccessLogQuery, AccessLogView};

/// 默认分页大小，与服务层保持一致。
const DEFAULT_LIMIT: i64 = 50;
/// 单次查询的硬上限，防止一次扫描过多历史数据。
const MAX_LIMIT: i64 = 200;

#[derive(Debug, thiserror::Error)]
pub enum AccessStoreError {
    #[error("访问日志 IO 失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("DuckDB 查询失败: {0}")]
    DuckDb(#[from] duckdb::Error),
    #[error("访问日志序列化失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("访问日志任务失败: {0}")]
    Join(String),
}

/// 访问日志文件句柄。
///
/// 克隆成本很低；同一文件的所有写入通过异步互斥锁串行化。
#[derive(Clone)]
pub struct AccessStore {
    path: PathBuf,
    write_lock: Arc<tokio::sync::Mutex<()>>,
    /// 已删除用户集合：阻止排队/重试中的事件把数据写回日志文件。
    deleted_users: Arc<tokio::sync::RwLock<std::collections::HashSet<String>>>,
}

/// 写入文件时使用的行结构；`id` 由 DuckDB 查询时按排序生成，不落盘。
#[derive(Serialize)]
struct StoredLog<'a> {
    session_id: &'a str,
    user_id: &'a str,
    entry_node_id: &'a str,
    target: &'a str,
    command: &'a str,
    started_at_ms: i64,
    ended_at_ms: i64,
    up_bytes: u64,
    down_bytes: u64,
    status: &'a str,
    error: &'a str,
    path: &'a [String],
    client_addr: &'a str,
}

impl AccessStore {
    /// 创建日志存储句柄；目录在首次写入时自动创建。
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
            deleted_users: Arc::new(tokio::sync::RwLock::new(std::collections::HashSet::new())),
        }
    }

    /// 标记用户已删除；之后到达的日志会被静默丢弃。
    pub async fn mark_users_deleted(&self, user_ids: &[String]) {
        let mut deleted = self.deleted_users.write().await;
        for id in user_ids {
            deleted.insert(id.clone());
        }
    }

    /// 删除失败时解除标记，避免误丢仍在使用的用户日志。
    pub async fn unmark_users_deleted(&self, user_ids: &[String]) {
        let mut deleted = self.deleted_users.write().await;
        for id in user_ids {
            deleted.remove(id);
        }
    }

    /// 取得“用户仍存在”的读租约。
    ///
    /// 调用方可在租约期间执行数据库写入（例如流量汇总）；删除流程获取写锁时
    /// 会等待租约释放，从而与删除标记共享同一个同步边界。
    pub async fn user_guard(
        &self,
        user_id: &str,
    ) -> Option<tokio::sync::OwnedRwLockReadGuard<std::collections::HashSet<String>>> {
        let guard = self.deleted_users.clone().read_owned().await;
        if guard.contains(user_id) {
            None
        } else {
            Some(guard)
        }
    }

    /// 追加一条访问日志；已删除用户的日志会被静默丢弃。
    pub async fn append(&self, log: &AccessLogInput) -> Result<(), AccessStoreError> {
        let mut buf = serialize_line(log)?.into_bytes();
        buf.push(b'\n');
        self.write_bytes(log, buf).await
    }

    /// 追加一批日志：只获取一次锁、打开一次文件、写入一次。
    ///
    /// 启动迁移会调用它，逐条 append 在百万行时会产生百万次 open/flush。
    pub async fn append_many(&self, logs: &[AccessLogInput]) -> Result<(), AccessStoreError> {
        if logs.is_empty() {
            return Ok(());
        }
        // 一次读锁内过滤掉已删除用户，再整批一次写入。
        let filtered = {
            let deleted = self.deleted_users.read().await;
            let mut buf = Vec::new();
            for log in logs {
                if deleted.contains(&log.user_id) {
                    continue;
                }
                buf.extend_from_slice(serialize_line(log)?.as_bytes());
                buf.push(b'\n');
            }
            buf
        };
        if filtered.is_empty() {
            return Ok(());
        }
        self.write_raw(filtered).await
    }

    /// 序列化后写入（单条日志使用）。
    async fn write_bytes(
        &self,
        log: &AccessLogInput,
        buf: Vec<u8>,
    ) -> Result<(), AccessStoreError> {
        // 先拿读锁再拿写文件锁，与 remove_users 的（写锁 → 文件锁）顺序一致，
        // 保证删除标记生效后不会再有新行写入。
        let deleted = self.deleted_users.read().await;
        if deleted.contains(&log.user_id) {
            return Ok(());
        }
        self.write_raw(buf).await
    }

    /// 在写锁内追加原始字节，并在失败时截断回写入前长度。
    async fn write_raw(&self, buf: Vec<u8>) -> Result<(), AccessStoreError> {
        let _guard = self.write_lock.lock().await;
        self.ensure_parent().await?;
        let mut options = tokio::fs::OpenOptions::new();
        options.create(true).append(true);
        let mut file = options.open(&self.path).await?;
        // 整批作为一次写入；失败时截回写入前长度，避免重试把两行拼在一起。
        let start = file.metadata().await?.len();
        let write = async {
            tokio::io::AsyncWriteExt::write_all(&mut file, &buf).await?;
            // flush 等待后台写入落到操作系统；不要求 fsync。
            tokio::io::AsyncWriteExt::flush(&mut file).await
        }
        .await;
        if let Err(err) = write {
            let _ = file.set_len(start).await;
            return Err(err.into());
        }
        // 持锁到写入完成，避免 prune 在后台写完成前替换文件。
        Ok(())
    }

    /// 按过滤条件查询一页日志。
    ///
    /// `visible_users` 为 `None` 表示管理员可见全部；`Some(empty)` 表示调用方
    /// 没有任何用户，直接返回空页而不是把条件拼成恒真。
    pub async fn query(
        &self,
        query: &AccessLogQuery,
        visible_users: Option<Vec<String>>,
    ) -> Result<AccessLogPage, AccessStoreError> {
        let limit = match query.limit {
            0 => DEFAULT_LIMIT,
            value => value.clamp(1, MAX_LIMIT),
        };
        let offset = query.offset.max(0);
        let path = self.path.clone();
        let query = query.clone();
        if !self.exists_non_empty().await? {
            return Ok(AccessLogPage {
                items: Vec::new(),
                total: 0,
                limit,
                offset,
            });
        }

        let result = tokio::task::spawn_blocking(move || {
            query_blocking(&path, &query, limit, offset, visible_users)
        })
        .await
        .map_err(|err| AccessStoreError::Join(err.to_string()))??;
        Ok(result)
    }

    /// 删除 `cutoff_ms` 之前的记录，返回删除的行数。
    pub async fn prune(&self, cutoff_ms: i64) -> Result<u64, AccessStoreError> {
        if !self.exists_non_empty().await? {
            return Ok(0);
        }
        let _guard = self.write_lock.lock().await;
        let path = self.path.clone();
        let pruned = tokio::task::spawn_blocking(move || {
            rewrite_file(&path, |value| {
                value
                    .get("started_at_ms")
                    .and_then(|item| item.as_i64())
                    .map(|started| started >= cutoff_ms)
                    .unwrap_or(false)
            })
        })
        .await
        .map_err(|err| AccessStoreError::Join(err.to_string()))??;
        Ok(pruned)
    }

    /// 删除指定用户的所有日志（账号/代理用户删除时调用）。
    ///
    /// 先写入删除标记再重写文件：标记生效后，排队中的事件不会再写回数据；
    /// 重写期间持有文件写锁，已有写入会先完成再被一并清理。
    pub async fn remove_users(&self, user_ids: &[String]) -> Result<u64, AccessStoreError> {
        if user_ids.is_empty() {
            return Ok(0);
        }
        // 只在插入标记期间持有写锁：之后的文件重写只需 write_lock，
        // 否则与被删用户无关的事件也会在整次重写期间被阻塞。
        {
            let mut deleted = self.deleted_users.write().await;
            for id in user_ids {
                deleted.insert(id.clone());
            }
        }
        if !self.exists_non_empty().await? {
            return Ok(0);
        }
        let _guard = self.write_lock.lock().await;
        let path = self.path.clone();
        let ids = user_ids.to_vec();
        let removed = tokio::task::spawn_blocking(move || {
            rewrite_file(&path, |value| {
                let user = value
                    .get("user_id")
                    .and_then(|item| item.as_str())
                    .unwrap_or("");
                !ids.iter().any(|id| id == user)
            })
        })
        .await
        .map_err(|err| AccessStoreError::Join(err.to_string()))??;
        Ok(removed)
    }

    async fn exists_non_empty(&self) -> Result<bool, std::io::Error> {
        match tokio::fs::metadata(&self.path).await {
            Ok(meta) => Ok(meta.len() > 0),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            // 权限或 I/O 故障不能伪装成空文件，向上传递让页面显示真实错误。
            Err(err) => Err(err),
        }
    }

    async fn ensure_parent(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        Ok(())
    }
}

/// 同步查询实现，在 blocking 线程中执行。
fn query_blocking(
    path: &Path,
    query: &AccessLogQuery,
    limit: i64,
    offset: i64,
    visible_users: Option<Vec<String>>,
) -> Result<AccessLogPage, AccessStoreError> {
    let conn = duckdb::Connection::open_in_memory()?;
    // 不使用 json 扩展（它可能触发网络下载），而是把 JSONL 读入内存表。
    // 过滤条件在流式读取时先应用，DuckDB 只处理匹配的行，避免每次查询
    // 都把所有历史记录插入内存表。
    load_logs(&conn, path, query, visible_users.as_deref())?;
    // 迁移重试或异常重试可能在文件中留下重复 session；按会话去重，
    // 保证分页与总数只统计一次。
    conn.execute_batch(
        "CREATE VIEW unique_logs AS
         SELECT * FROM (
             SELECT *, row_number() OVER (
                 PARTITION BY \"session_id\" ORDER BY \"started_at_ms\" DESC
             ) AS rn
             FROM logs
         ) WHERE rn = 1",
    )?;

    let total: i64 = conn.query_row("SELECT COUNT(*) FROM unique_logs", [], |row| row.get(0))?;

    let sql = format!(
        "SELECT row_number() OVER (ORDER BY \"started_at_ms\" DESC, \"session_id\") AS id,
                \"session_id\", \"user_id\", \"entry_node_id\", \"target\", \"command\",
                \"started_at_ms\", \"ended_at_ms\",
                (\"ended_at_ms\" - \"started_at_ms\") AS duration_ms,
                \"up_bytes\", \"down_bytes\", \"status\", \"error\",
                \"path\" AS path_json, \"client_addr\"
         FROM unique_logs
         ORDER BY \"started_at_ms\" DESC, \"session_id\"
         LIMIT {limit} OFFSET {offset}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        let path_json: String = row.get(13)?;
        let path: Vec<String> = serde_json::from_str(&path_json).unwrap_or_default();
        items.push(AccessLogView {
            id: row.get(0)?,
            session_id: row.get(1)?,
            user_id: row.get(2)?,
            entry_node_id: row.get(3)?,
            target: row.get(4)?,
            command: row.get(5)?,
            started_at: row.get(6)?,
            ended_at: row.get(7)?,
            duration_ms: row.get(8)?,
            up_bytes: row.get::<_, i64>(9)? as u64,
            down_bytes: row.get::<_, i64>(10)? as u64,
            status: row.get(11)?,
            error: row.get(12)?,
            path,
            client_addr: row.get(14)?,
        });
    }
    Ok(AccessLogPage {
        items,
        total,
        limit,
        offset,
    })
}

/// 把一条日志序列化为一行 JSON（不带换行）。
fn serialize_line(log: &AccessLogInput) -> Result<String, serde_json::Error> {
    serde_json::to_string(&StoredLog {
        session_id: &log.session_id,
        user_id: &log.user_id,
        entry_node_id: &log.entry_node_id,
        target: &log.target,
        command: &log.command,
        started_at_ms: log.started_at_ms,
        ended_at_ms: log.ended_at_ms,
        up_bytes: log.up_bytes,
        down_bytes: log.down_bytes,
        status: &log.status,
        error: &log.error,
        path: &log.path,
        client_addr: &log.client_addr,
    })
}

/// 把 JSONL 文件加载进内存表；过滤条件在这里先应用，减少进入 DuckDB 的行数。
fn load_logs(
    conn: &duckdb::Connection,
    path: &Path,
    query: &AccessLogQuery,
    visible_users: Option<&[String]>,
) -> Result<(), AccessStoreError> {
    conn.execute_batch(
        "CREATE TABLE logs (
            session_id VARCHAR,
            user_id VARCHAR,
            entry_node_id VARCHAR,
            target VARCHAR,
            command VARCHAR,
            started_at_ms BIGINT,
            ended_at_ms BIGINT,
            up_bytes BIGINT,
            down_bytes BIGINT,
            status VARCHAR,
            error VARCHAR,
            path VARCHAR,
            client_addr VARCHAR
        )",
    )?;

    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut insert =
        conn.prepare("INSERT INTO logs VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
    // 单事务批量插入：避免每条 INSERT 单独提交，百万行时差异可达数量级。
    conn.execute_batch("BEGIN")?;
    let loaded = (|| -> Result<(), AccessStoreError> {
        for line in std::io::BufRead::lines(reader) {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                // 损坏的行跳过而不是让整页查询失败。
                continue;
            };
            if !matches_query(&value, query, visible_users) {
                continue;
            }
            let text = |key: &str| {
                value
                    .get(key)
                    .and_then(|item| item.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let number = |key: &str| value.get(key).and_then(|item| item.as_i64()).unwrap_or(0);
            let path_json = serde_json::to_string(&value.get("path").cloned().unwrap_or_default())
                .unwrap_or_else(|_| "[]".into());
            insert.execute(duckdb::params![
                text("session_id"),
                text("user_id"),
                text("entry_node_id"),
                text("target"),
                text("command"),
                number("started_at_ms"),
                number("ended_at_ms"),
                number("up_bytes"),
                number("down_bytes"),
                text("status"),
                text("error"),
                path_json,
                text("client_addr"),
            ])?;
        }
        Ok(())
    })();
    match loaded {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// 判断一行日志是否满足查询条件（与 REST/RPC 语义一致）。
fn matches_query(
    value: &serde_json::Value,
    query: &AccessLogQuery,
    visible_users: Option<&[String]>,
) -> bool {
    let text = |key: &str| value.get(key).and_then(|item| item.as_str()).unwrap_or("");
    if let Some(user_id) = query.user_id.as_deref() {
        if text("user_id") != user_id {
            return false;
        }
    }
    if let Some(node_id) = query.node_id.as_deref() {
        if text("entry_node_id") != node_id {
            return false;
        }
    }
    if let Some(status) = query.status.as_deref() {
        if text("status") != status {
            return false;
        }
    }
    if let Some(target) = query.target.as_deref().filter(|value| !value.is_empty()) {
        if !text("target").contains(target) {
            return false;
        }
    }
    let started = value
        .get("started_at_ms")
        .and_then(|item| item.as_i64())
        .unwrap_or(0);
    if let Some(from) = query.from {
        if started < from {
            return false;
        }
    }
    if let Some(to) = query.to {
        if started > to {
            return false;
        }
    }
    if let Some(users) = visible_users {
        if users.is_empty() || !users.iter().any(|user| user == text("user_id")) {
            return false;
        }
    }
    true
}

/// 逐行过滤并原子替换文件，返回删除的行数。
fn rewrite_file(
    path: &Path,
    keep: impl Fn(&serde_json::Value) -> bool,
) -> Result<u64, AccessStoreError> {
    use std::io::{BufRead, BufReader, BufWriter, Write};

    let input = std::fs::File::open(path)?;
    let temp_path = path.with_extension("jsonl.tmp");
    let output = std::fs::File::create(&temp_path)?;
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);
    let mut removed = 0u64;
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let keep_line = serde_json::from_str::<serde_json::Value>(line.trim())
            .map(|value| keep(&value))
            .unwrap_or(false);
        if keep_line {
            writer.write_all(line.as_bytes())?;
        } else {
            removed += 1;
        }
    }
    writer.flush()?;
    drop(writer);
    std::fs::rename(&temp_path, path)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(started_at_ms: i64, user: &str) -> AccessLogInput {
        AccessLogInput {
            session_id: format!("session-{started_at_ms}"),
            user_id: user.into(),
            entry_node_id: "node-1".into(),
            target: "example.com:443".into(),
            command: "tcp".into(),
            started_at_ms,
            ended_at_ms: started_at_ms + 500,
            up_bytes: 10,
            down_bytes: 20,
            status: "ok".into(),
            error: String::new(),
            path: vec!["node-1".into()],
            client_addr: "203.0.113.1:1234".into(),
        }
    }

    async fn store() -> (AccessStore, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let store = AccessStore::new(dir.path().join("logs.jsonl"));
        (store, dir)
    }

    #[tokio::test]
    async fn append_and_query_round_trip() {
        let (store, _dir) = store().await;
        store.append(&log(1_000, "alice")).await.unwrap();
        store.append(&log(2_000, "bob")).await.unwrap();
        store.append(&log(3_000, "alice")).await.unwrap();

        let page = store
            .query(
                &AccessLogQuery {
                    limit: 50,
                    offset: 0,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 3);
        // 按时间倒序，最新一条在最前。
        assert_eq!(page.items[0].started_at, 3_000);
        assert_eq!(page.items[0].duration_ms, 500);
        assert_eq!(page.items[0].path, vec!["node-1".to_string()]);
    }

    #[tokio::test]
    async fn empty_allowlist_denies_all_rows() {
        let (store, _dir) = store().await;
        store.append(&log(1_000, "alice")).await.unwrap();

        // 普通账号没有任何代理用户时，白名单为空必须拒绝全部记录，
        // 不能被解释成「不过滤」。
        let page = store
            .query(
                &AccessLogQuery {
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                Some(vec![]),
            )
            .await
            .unwrap();
        assert_eq!(page.total, 0);
        assert!(page.items.is_empty());
    }

    #[tokio::test]
    async fn filters_and_visibility_are_applied() {
        let (store, _dir) = store().await;
        store.append(&log(1_000, "alice")).await.unwrap();
        store.append(&log(2_000, "bob")).await.unwrap();

        let page = store
            .query(
                &AccessLogQuery {
                    status: Some("ok".into()),
                    user_id: Some("bob".into()),
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                Some(vec!["bob".into()]),
            )
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].user_id, "bob");

        let hidden = store
            .query(
                &AccessLogQuery {
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                Some(vec![]),
            )
            .await
            .unwrap();
        assert_eq!(hidden.total, 0);
    }

    #[tokio::test]
    async fn pagination_and_clamping() {
        let (store, _dir) = store().await;
        for index in 0..5 {
            store.append(&log(1_000 + index, "alice")).await.unwrap();
        }
        let page = store
            .query(
                &AccessLogQuery {
                    limit: 100_000,
                    offset: 2,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.limit, MAX_LIMIT);
        assert_eq!(page.offset, 2);
        assert_eq!(page.items.len(), 3);
    }

    #[tokio::test]
    async fn prune_removes_old_lines() {
        let (store, _dir) = store().await;
        store.append(&log(1_000, "alice")).await.unwrap();
        store.append(&log(5_000, "alice")).await.unwrap();

        let removed = store.prune(3_000).await.unwrap();
        assert_eq!(removed, 1);
        let page = store
            .query(
                &AccessLogQuery {
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].started_at, 5_000);
    }

    #[tokio::test]
    async fn remove_users_purges_matching_rows() {
        let (store, _dir) = store().await;
        store.append(&log(1_000, "alice")).await.unwrap();
        store.append(&log(2_000, "bob")).await.unwrap();

        let removed = store.remove_users(&["alice".to_string()]).await.unwrap();
        assert_eq!(removed, 1);

        let page = store
            .query(
                &AccessLogQuery {
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].user_id, "bob");
    }

    #[tokio::test]
    async fn duplicate_sessions_are_counted_once() {
        let (store, _dir) = store().await;
        // 同一条会话重复写入（例如迁移重试），查询应只返回一条。
        store.append(&log(1_000, "alice")).await.unwrap();
        store.append(&log(1_000, "alice")).await.unwrap();

        let page = store
            .query(
                &AccessLogQuery {
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
    }

    #[tokio::test]
    async fn removal_blocks_late_appends() {
        let (store, _dir) = store().await;
        store.append(&log(1_000, "alice")).await.unwrap();

        let writer = store.clone();
        let write = async move {
            for offset in 0..5 {
                writer.append(&log(2_000 + offset, "alice")).await.unwrap();
            }
        };
        // 删除与追加并发：删除标记 + 写锁保证最终不残留该用户。
        let ids = ["alice".to_string()];
        let (_, removed) = tokio::join!(write, store.remove_users(&ids));
        assert!(removed.is_ok());

        let page = store
            .query(
                &AccessLogQuery {
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 0);
    }

    #[tokio::test]
    async fn rewrite_failure_keeps_original_file() {
        let (store, dir) = store().await;
        store.append(&log(1_000, "alice")).await.unwrap();

        // 在临时文件路径上放一个目录，使 prune 的 File::create 失败。
        let temp = dir.path().join("logs.jsonl.tmp");
        std::fs::create_dir(&temp).unwrap();
        assert!(store.prune(9_999).await.is_err());

        let page = store
            .query(
                &AccessLogQuery {
                    limit: 10,
                    offset: 0,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 1, "失败的重写不能破坏原文件");
        std::fs::remove_dir(&temp).unwrap();
    }

    #[tokio::test]
    async fn missing_file_returns_empty_page() {
        let (store, _dir) = store().await;
        let page = store
            .query(
                &AccessLogQuery {
                    limit: 0,
                    offset: -1,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(page.total, 0);
        assert_eq!(page.limit, DEFAULT_LIMIT);
        assert_eq!(page.offset, 0);
    }
}

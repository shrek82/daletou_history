use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::error::DaletouError;
use crate::types::DrawRecord;

/// 数据库配置
#[derive(Clone)]
pub struct DbConfig {
    /// 多久爬取一次第一页（秒），默认 3600（1小时）
    pub crawl_interval: Duration,
    /// 最多保留多少条历史记录，默认 365
    pub max_records: usize,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            crawl_interval: Duration::from_secs(3600),
            max_records: 365,
        }
    }
}

/// SQLite 数据库客户端
#[derive(Clone)]
pub struct DbClient {
    conn: Arc<Mutex<Connection>>,
    config: DbConfig,
}

impl DbClient {
    /// 打开或创建数据库，自动建表
    pub fn new(path: &Path) -> Result<Self, DaletouError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DaletouError::ParseError(format!("创建数据库目录失败: {}", e)))?;
        }

        let conn = Connection::open(path)
            .map_err(|e| DaletouError::ParseError(format!("打开数据库失败: {}", e)))?;

        // 建表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS draw_records (
                issue TEXT PRIMARY KEY,
                date TEXT NOT NULL,
                weekday TEXT NOT NULL,
                red_1 INTEGER NOT NULL,
                red_2 INTEGER NOT NULL,
                red_3 INTEGER NOT NULL,
                red_4 INTEGER NOT NULL,
                red_5 INTEGER NOT NULL,
                blue_1 INTEGER NOT NULL,
                blue_2 INTEGER NOT NULL,
                prize_pool TEXT NOT NULL,
                crawled_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(db_error)?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_draw_records_date ON draw_records(date DESC)",
            [],
        )
        .map_err(db_error)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )
        .map_err(db_error)?;

        Ok(Self { conn: Arc::new(Mutex::new(conn)), config: DbConfig::default() })
    }

    /// 为后台自动更新线程创建克隆（共享同一个数据库连接）
    pub(crate) fn clone_for_auto_update(&self) -> Self {
        self.clone()
    }

    /// 设置配置
    pub fn with_config(mut self, config: DbConfig) -> Self {
        self.config = config;
        self
    }

    /// 获取配置引用
    pub fn config(&self) -> &DbConfig {
        &self.config
    }

    /// 检查是否到了爬取时间
    pub fn should_crawl(&self) -> bool {
        let last = self.get_config_u64("first_page_crawl_at").unwrap_or(0);
        let now = now_secs();
        now.saturating_sub(last) >= self.config.crawl_interval.as_secs()
    }

    /// 将一批开奖记录插入数据库（INSERT OR IGNORE，按 issue 去重）
    pub fn update_latest(&self, records: &[DrawRecord]) -> Result<(), DaletouError> {
        let now = now_secs();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(db_error)?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO draw_records
                 (issue, date, weekday, red_1, red_2, red_3, red_4, red_5,
                  blue_1, blue_2, prize_pool, crawled_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )
            .map_err(db_error)?;

            for r in records {
                if r.balls.red.len() < 5 || r.balls.blue.len() < 2 {
                    continue;
                }
                stmt.execute((
                    &r.issue,
                    &r.date,
                    &r.weekday,
                    r.balls.red[0] as i32,
                    r.balls.red[1] as i32,
                    r.balls.red[2] as i32,
                    r.balls.red[3] as i32,
                    r.balls.red[4] as i32,
                    r.balls.blue[0] as i32,
                    r.balls.blue[1] as i32,
                    &r.prize_pool,
                    now,
                ))
                .map_err(db_error)?;
            }
        }

        // 更新爬取时间戳
        tx.execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES ('first_page_crawl_at', ?1)",
            [now.to_string()],
        )
        .map_err(db_error)?;

        tx.commit().map_err(db_error)?;

        // 修剪到最大条数
        trim_to_max_on_conn(&conn, self.config.max_records)?;

        Ok(())
    }

    /// 从数据库查询最新 N 条记录（按 date DESC）
    pub fn get_latest_n(&self, n: usize) -> Result<Vec<DrawRecord>, DaletouError> {
        let conn = self.conn.lock().unwrap();
        Self::query_records(&conn, 0, n)
    }

    /// 分页查询开奖记录（按 date DESC）
    ///
    /// page 从 1 开始，page_size 为每页条数。
    /// 返回 (总记录数, 当前页记录列表)。
    pub fn get_page_records(&self, page: u32, page_size: usize) -> Result<(usize, Vec<DrawRecord>), DaletouError> {
        let conn = self.conn.lock().unwrap();
        let total = Self::query_count(&conn)?;
        let offset = ((page - 1) as usize) * page_size;
        let records = Self::query_records(&conn, offset, page_size)?;
        Ok((total, records))
    }

    /// 按期号查询
    pub fn get_by_issue(&self, issue: &str) -> Result<Option<DrawRecord>, DaletouError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT issue, date, weekday,
                    red_1, red_2, red_3, red_4, red_5,
                    blue_1, blue_2, prize_pool
             FROM draw_records
             WHERE issue = ?1",
        )
        .map_err(db_error)?;

        let row = stmt.query_row([issue], |row| {
            Ok(DrawRecord {
                issue: row.get(0)?,
                date: row.get(1)?,
                weekday: row.get(2)?,
                balls: crate::types::BallSet {
                    red: vec![
                        row.get::<_, i32>(3)? as u8,
                        row.get::<_, i32>(4)? as u8,
                        row.get::<_, i32>(5)? as u8,
                        row.get::<_, i32>(6)? as u8,
                        row.get::<_, i32>(7)? as u8,
                    ],
                    blue: vec![
                        row.get::<_, i32>(8)? as u8,
                        row.get::<_, i32>(9)? as u8,
                    ],
                },
                prize_pool: row.get(10)?,
            })
        });

        match row {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(db_error(e)),
        }
    }

    /// 当前数据库中的记录数
    pub fn count(&self) -> Result<usize, DaletouError> {
        let conn = self.conn.lock().unwrap();
        Self::query_count(&conn)
    }

    /// 保留最新的 max 条，删除旧的
    pub fn trim_to_max(&self, max: usize) -> Result<(), DaletouError> {
        let conn = self.conn.lock().unwrap();
        trim_to_max_on_conn(&conn, max)
    }

    /// 获取配置值（字符串）
    fn get_config(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM config WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()
    }

    /// 获取配置值（u64）
    fn get_config_u64(&self, key: &str) -> Option<u64> {
        self.get_config(key).and_then(|v| v.parse().ok())
    }

    /// 通用查询：按 offset + limit 分页（date DESC）
    fn query_records(conn: &Connection, offset: usize, limit: usize) -> Result<Vec<DrawRecord>, DaletouError> {
        let mut stmt = conn.prepare(
            "SELECT issue, date, weekday,
                    red_1, red_2, red_3, red_4, red_5,
                    blue_1, blue_2, prize_pool
             FROM draw_records
             ORDER BY date DESC
             LIMIT ?1 OFFSET ?2",
        )
        .map_err(db_error)?;

        let rows = stmt
            .query_map(rusqlite::params![limit as i32, offset as i32], |row| {
                Ok(DrawRecord {
                    issue: row.get(0)?,
                    date: row.get(1)?,
                    weekday: row.get(2)?,
                    balls: crate::types::BallSet {
                        red: vec![
                            row.get::<_, i32>(3)? as u8,
                            row.get::<_, i32>(4)? as u8,
                            row.get::<_, i32>(5)? as u8,
                            row.get::<_, i32>(6)? as u8,
                            row.get::<_, i32>(7)? as u8,
                        ],
                        blue: vec![
                            row.get::<_, i32>(8)? as u8,
                            row.get::<_, i32>(9)? as u8,
                        ],
                    },
                    prize_pool: row.get(10)?,
                })
            })
            .map_err(db_error)?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(db_error)?);
        }

        Ok(result)
    }

    /// 查询总记录数
    fn query_count(conn: &Connection) -> Result<usize, DaletouError> {
        let c: i32 = conn
            .query_row("SELECT COUNT(*) FROM draw_records", [], |row| row.get(0))
            .map_err(db_error)?;
        Ok(c as usize)
    }
}

fn trim_to_max_on_conn(conn: &Connection, max: usize) -> Result<(), DaletouError> {
    conn.execute(
        "DELETE FROM draw_records
         WHERE issue IN (
             SELECT issue FROM draw_records
             ORDER BY date DESC
             LIMIT -1 OFFSET ?1
         )",
        [max as i32],
    )
    .map_err(db_error)?;
    Ok(())
}

fn db_error(err: rusqlite::Error) -> DaletouError {
    DaletouError::ParseError(format!("数据库操作失败: {}", err))
}

/// 当前时间戳（秒）
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BallSet;
    use std::fs;

    fn temp_db_path() -> std::path::PathBuf {
        // 使用 target/ 目录避免 /tmp 权限问题
        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("target");
        path.push("test_tmp");
        let _ = std::fs::create_dir_all(&path);
        path.push(format!("daletou_test_{}.db", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
        path
    }

    #[test]
    fn test_create_and_insert() {
        let path = temp_db_path();
        let _guard = DropGuard(path.clone());

        let db = DbClient::new(&path).unwrap();
        let records = vec![
            DrawRecord {
                issue: "26001".to_string(),
                date: "2026-01-01".to_string(),
                weekday: "四".to_string(),
                balls: BallSet {
                    red: vec![1, 2, 3, 4, 5],
                    blue: vec![6, 7],
                },
                prize_pool: "1000万".to_string(),
            },
        ];
        db.update_latest(&records).unwrap();
        assert_eq!(db.count().unwrap(), 1);

        let fetched = db.get_latest_n(10).unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].issue, "26001");
        assert_eq!(fetched[0].balls.red, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_duplicate_insert() {
        let path = temp_db_path();
        let _guard = DropGuard(path.clone());

        let db = DbClient::new(&path).unwrap();
        let record = DrawRecord {
            issue: "26001".to_string(),
            date: "2026-01-01".to_string(),
            weekday: "四".to_string(),
            balls: BallSet {
                red: vec![1, 2, 3, 4, 5],
                blue: vec![6, 7],
            },
            prize_pool: "1000万".to_string(),
        };
        db.update_latest(&[record.clone()]).unwrap();
        // 重复插入相同 issue，应被 IGNORE
        db.update_latest(&[record]).unwrap();
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn test_trim_to_max() {
        let path = temp_db_path();
        let _guard = DropGuard(path.clone());

        let db = DbClient::new(&path).unwrap();
        let mut records = Vec::new();
        for i in 1..=10 {
            records.push(DrawRecord {
                issue: format!("26{:03}", i),
                date: format!("2026-01-{:02}", i),
                weekday: "四".to_string(),
                balls: BallSet {
                    red: vec![i, i + 1, i + 2, i + 3, i + 4],
                    blue: vec![i % 12 + 1, i % 12 + 2],
                },
                prize_pool: "1000万".to_string(),
            });
        }
        db.update_latest(&records).unwrap();
        assert_eq!(db.count().unwrap(), 10);

        // 设置 max_records=5，触发 trim
        let config = DbConfig {
            crawl_interval: Duration::from_secs(3600),
            max_records: 5,
        };
        let db = db.with_config(config);
        let new_records = vec![
            DrawRecord {
                issue: "26011".to_string(),
                date: "2026-01-11".to_string(),
                weekday: "四".to_string(),
                balls: BallSet {
                    red: vec![11, 12, 13, 14, 15],
                    blue: vec![1, 2],
                },
                prize_pool: "1000万".to_string(),
            },
        ];
        db.update_latest(&new_records).unwrap();
        assert_eq!(db.count().unwrap(), 5);
    }

    #[test]
    fn test_should_crawl() {
        let path = temp_db_path();
        let _guard = DropGuard(path.clone());

        let config = DbConfig {
            crawl_interval: Duration::from_secs(0), // 总是需要爬取
            max_records: 10000,
        };
        let db = DbClient::new(&path).unwrap().with_config(config);
        assert!(db.should_crawl());

        // 首次爬取后
        let record = DrawRecord {
            issue: "26001".to_string(),
            date: "2026-01-01".to_string(),
            weekday: "四".to_string(),
            balls: BallSet {
                red: vec![1, 2, 3, 4, 5],
                blue: vec![6, 7],
            },
            prize_pool: "1000万".to_string(),
        };
        db.update_latest(&[record]).unwrap();

        // 间隔为0，仍然应该爬取
        assert!(db.should_crawl());

        // 设置间隔为1小时
        let config2 = DbConfig {
            crawl_interval: Duration::from_secs(3600),
            max_records: 10000,
        };
        // 重新打开数据库
        let db2 = DbClient::new(&path).unwrap().with_config(config2);
        assert!(!db2.should_crawl()); // 刚爬取过，不应再爬
    }

    #[test]
    fn test_get_page_records() {
        let path = temp_db_path();
        let _guard = DropGuard(path.clone());

        let db = DbClient::new(&path).unwrap();
        let mut records = Vec::new();
        for i in 1..=10 {
            records.push(DrawRecord {
                issue: format!("26{:03}", i),
                date: format!("2026-01-{:02}", i),
                weekday: "四".to_string(),
                balls: BallSet {
                    red: vec![i, i + 1, i + 2, i + 3, i + 4],
                    blue: vec![i % 12 + 1, i % 12 + 2],
                },
                prize_pool: "1000万".to_string(),
            });
        }
        db.update_latest(&records).unwrap();

        // 第1页，每页3条
        let (total, page_records) = db.get_page_records(1, 3).unwrap();
        assert_eq!(total, 10);
        assert_eq!(page_records.len(), 3);
        // date DESC，所以第1条是 26010
        assert_eq!(page_records[0].issue, "26010");

        // 第2页
        let (total, page_records) = db.get_page_records(2, 3).unwrap();
        assert_eq!(total, 10);
        assert_eq!(page_records.len(), 3);
        assert_eq!(page_records[0].issue, "26007");

        // 第4页（最后一页，只有1条）
        let (total, page_records) = db.get_page_records(4, 3).unwrap();
        assert_eq!(total, 10);
        assert_eq!(page_records.len(), 1);
        assert_eq!(page_records[0].issue, "26001");
    }

    #[test]
    fn test_get_by_issue() {
        let path = temp_db_path();
        let _guard = DropGuard(path.clone());

        let db = DbClient::new(&path).unwrap();
        let record = DrawRecord {
            issue: "26001".to_string(),
            date: "2026-01-01".to_string(),
            weekday: "四".to_string(),
            balls: BallSet {
                red: vec![1, 2, 3, 4, 5],
                blue: vec![6, 7],
            },
            prize_pool: "1000万".to_string(),
        };
        db.update_latest(&[record]).unwrap();

        // 存在
        let found = db.get_by_issue("26001").unwrap();
        assert!(found.is_some());
        let r = found.unwrap();
        assert_eq!(r.issue, "26001");
        assert_eq!(r.balls.red, vec![1, 2, 3, 4, 5]);

        // 不存在
        let found = db.get_by_issue("99999").unwrap();
        assert!(found.is_none());
    }

    struct DropGuard(std::path::PathBuf);
    impl Drop for DropGuard {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
}

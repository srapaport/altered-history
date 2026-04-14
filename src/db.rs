use anyhow::{Context, Result};
use dotenv::dotenv;
use regex::Regex;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashSet;

use crate::env;

/// Holds the suffixed table names derived from the graph timestamp.
/// e.g. for graph path `.../graph/2025-05-18/compressed/graph`,
/// tables become `altered_histories_2025_05_18`, `checkpoints_2025_05_18`.
#[derive(Debug, Clone)]
pub struct TableNames {
    pub altered_histories: String,
    pub checkpoints: String,
}

/// Extracts a `YYYY_MM_DD` suffix from a graph path containing a date segment
/// like `2025-05-18`. Falls back to an empty suffix if no date is found.
pub fn extract_graph_suffix(graph_path: &str) -> String {
    let re = Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap();
    if let Some(caps) = re.captures(graph_path) {
        format!("{}_{}_{}",
            caps.get(1).unwrap().as_str(),
            caps.get(2).unwrap().as_str(),
            caps.get(3).unwrap().as_str(),
        )
    } else {
        String::new()
    }
}

impl TableNames {
    pub fn from_graph_path(graph_path: &str) -> Self {
        let suffix = extract_graph_suffix(graph_path);
        if suffix.is_empty() {
            TableNames {
                altered_histories: "altered_histories".to_string(),
                checkpoints: "checkpoints".to_string(),
            }
        } else {
            TableNames {
                altered_histories: format!("altered_histories_{}", suffix),
                checkpoints: format!("checkpoints_{}", suffix),
            }
        }
    }
}

pub async fn init_pool() -> Result<PgPool> {
    dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .context("Failed to connect to PostgreSQL")?;
    Ok(pool)
}

pub async fn create_tables(pool: &PgPool, tables: &TableNames) -> Result<()> {
    let q = format!(
        "CREATE TABLE IF NOT EXISTS {} (
            id BIGSERIAL PRIMARY KEY,
            origin TEXT NOT NULL,
            snapshot_src TEXT NOT NULL,
            branch_name TEXT NOT NULL,
            missing_commit TEXT NOT NULL,
            snapshot_dst TEXT NOT NULL,
            first_difference TEXT,
            main_category TEXT,
            sub_categories TEXT,
            status TEXT NOT NULL DEFAULT 'detected'
        )",
        tables.altered_histories
    );
    sqlx::query(&q)
        .execute(pool)
        .await
        .context("Failed to create altered_histories table")?;

    let idx = format!(
        "CREATE INDEX IF NOT EXISTS idx_{t}_origin ON {t} (origin)",
        t = tables.altered_histories
    );
    sqlx::query(&idx).execute(pool).await?;
    let idx = format!(
        "CREATE INDEX IF NOT EXISTS idx_{t}_missing_commit ON {t} (missing_commit)",
        t = tables.altered_histories
    );
    sqlx::query(&idx).execute(pool).await?;
    let idx = format!(
        "CREATE INDEX IF NOT EXISTS idx_{t}_status ON {t} (status)",
        t = tables.altered_histories
    );
    sqlx::query(&idx).execute(pool).await?;

    let q = format!(
        "CREATE TABLE IF NOT EXISTS {} (
            origin_swhid TEXT PRIMARY KEY
        )",
        tables.checkpoints
    );
    sqlx::query(&q)
        .execute(pool)
        .await
        .context("Failed to create checkpoints table")?;

    println!(
        "Database tables created or verified: {}, {}",
        tables.altered_histories, tables.checkpoints
    );
    Ok(())
}

pub async fn batch_insert_altered_commits(
    pool: &PgPool,
    tables: &TableNames,
    records: &[env::AlteredCommit],
) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }

    let q = format!(
        "INSERT INTO {} (origin, snapshot_src, branch_name, missing_commit, snapshot_dst)
         SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::text[])",
        tables.altered_histories
    );

    for chunk in records.chunks(1000) {
        let mut origins = Vec::with_capacity(chunk.len());
        let mut snapshot_srcs = Vec::with_capacity(chunk.len());
        let mut branch_names = Vec::with_capacity(chunk.len());
        let mut missing_commits = Vec::with_capacity(chunk.len());
        let mut snapshot_dsts = Vec::with_capacity(chunk.len());

        for record in chunk {
            origins.push(record.origin.as_str());
            snapshot_srcs.push(record.snapshot_src.as_str());
            branch_names.push(record.branch_name.as_str());
            missing_commits.push(record.missing_commit.as_str());
            snapshot_dsts.push(record.snapshot_dst.as_str());
        }

        sqlx::query(&q)
            .bind(&origins)
            .bind(&snapshot_srcs)
            .bind(&branch_names)
            .bind(&missing_commits)
            .bind(&snapshot_dsts)
            .execute(pool)
            .await
            .context("Failed to batch insert altered commits")?;
    }

    Ok(())
}

pub async fn insert_checkpoints(
    pool: &PgPool,
    tables: &TableNames,
    swhids: &HashSet<String>,
) -> Result<()> {
    if swhids.is_empty() {
        return Ok(());
    }

    let q = format!(
        "INSERT INTO {} (origin_swhid)
         SELECT * FROM UNNEST($1::text[])
         ON CONFLICT (origin_swhid) DO NOTHING",
        tables.checkpoints
    );

    let swhids_vec: Vec<&str> = swhids.iter().map(|s| s.as_str()).collect();

    for chunk in swhids_vec.chunks(1000) {
        let chunk_vec: Vec<&str> = chunk.to_vec();
        sqlx::query(&q)
            .bind(&chunk_vec)
            .execute(pool)
            .await
            .context("Failed to insert checkpoints")?;
    }

    Ok(())
}

pub async fn load_checkpoints(pool: &PgPool, tables: &TableNames) -> Result<HashSet<String>> {
    let q = format!("SELECT origin_swhid FROM {}", tables.checkpoints);
    let rows: Vec<(String,)> = sqlx::query_as(&q)
        .fetch_all(pool)
        .await
        .context("Failed to load checkpoints")?;

    Ok(rows.into_iter().map(|(s,)| s).collect())
}

pub async fn load_detected_commits(
    pool: &PgPool,
    tables: &TableNames,
) -> Result<HashSet<(String, String, String, String, String)>> {
    let q = format!(
        "SELECT origin, snapshot_src, branch_name, missing_commit, snapshot_dst
         FROM {} WHERE status = 'detected'",
        tables.altered_histories
    );
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(&q)
        .fetch_all(pool)
        .await
        .context("Failed to load detected commits")?;

    Ok(rows.into_iter().collect())
}

pub async fn promote_to_root_cause(
    pool: &PgPool,
    tables: &TableNames,
    root_cause_commits: &HashSet<(String, String, String, String, String)>,
) -> Result<()> {
    let q_update = format!(
        "UPDATE {} SET status = 'root_cause'
         WHERE status = 'detected' AND missing_commit = ANY($1::text[])",
        tables.altered_histories
    );
    let q_delete = format!(
        "DELETE FROM {} WHERE status = 'detected'",
        tables.altered_histories
    );

    let missing_commits: Vec<&str> = root_cause_commits
        .iter()
        .map(|r| r.3.as_str())
        .collect();

    for chunk in missing_commits.chunks(1000) {
        let chunk_vec: Vec<&str> = chunk.to_vec();
        sqlx::query(&q_update)
            .bind(&chunk_vec)
            .execute(pool)
            .await
            .context("Failed to promote commits to root_cause")?;
    }

    sqlx::query(&q_delete)
        .execute(pool)
        .await
        .context("Failed to delete non-root-cause commits")?;

    Ok(())
}

pub async fn load_root_cause_commits(
    pool: &PgPool,
    tables: &TableNames,
) -> Result<Vec<(String, String, String, String, String)>> {
    let q = format!(
        "SELECT origin, snapshot_src, branch_name, missing_commit, snapshot_dst
         FROM {} WHERE status = 'root_cause'",
        tables.altered_histories
    );
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(&q)
        .fetch_all(pool)
        .await
        .context("Failed to load root_cause commits")?;

    Ok(rows)
}

pub async fn batch_update_classification(
    pool: &PgPool,
    tables: &TableNames,
    updates: &[(String, String, String, String, String, Option<String>, String, String)],
) -> Result<()> {
    if updates.is_empty() {
        return Ok(());
    }

    let q = format!(
        "UPDATE {}
         SET first_difference = $1, main_category = $2, sub_categories = $3, status = 'classified'
         WHERE origin = $4 AND snapshot_src = $5 AND branch_name = $6
           AND missing_commit = $7 AND snapshot_dst = $8 AND status = 'root_cause'",
        tables.altered_histories
    );

    for chunk in updates.chunks(500) {
        let mut tx = pool.begin().await?;
        for (origin, snapshot_src, branch_name, missing_commit, snapshot_dst, first_diff, main_cat, sub_cats) in chunk {
            sqlx::query(&q)
                .bind(first_diff.as_deref())
                .bind(main_cat)
                .bind(sub_cats)
                .bind(origin)
                .bind(snapshot_src)
                .bind(branch_name)
                .bind(missing_commit)
                .bind(snapshot_dst)
                .execute(&mut *tx)
                .await
                .context("Failed to update classification in batch")?;
        }
        tx.commit().await?;
    }

    Ok(())
}

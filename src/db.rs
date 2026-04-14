use anyhow::{Context, Result};
use dotenv::dotenv;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashSet;

use crate::env;

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

pub async fn create_tables(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS altered_histories (
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
    )
    .execute(pool)
    .await
    .context("Failed to create altered_histories table")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_ah_origin ON altered_histories (origin)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_ah_missing_commit ON altered_histories (missing_commit)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_ah_status ON altered_histories (status)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS checkpoints (
            origin_swhid TEXT PRIMARY KEY
        )",
    )
    .execute(pool)
    .await
    .context("Failed to create checkpoints table")?;

    println!("Database tables created or verified.");
    Ok(())
}

pub async fn batch_insert_altered_commits(
    pool: &PgPool,
    records: &[env::AlteredCommit],
) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }

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

        sqlx::query(
            "INSERT INTO altered_histories (origin, snapshot_src, branch_name, missing_commit, snapshot_dst)
             SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::text[])",
        )
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

pub async fn insert_checkpoints(pool: &PgPool, swhids: &HashSet<String>) -> Result<()> {
    if swhids.is_empty() {
        return Ok(());
    }

    let swhids_vec: Vec<&str> = swhids.iter().map(|s| s.as_str()).collect();

    for chunk in swhids_vec.chunks(1000) {
        let chunk_vec: Vec<&str> = chunk.to_vec();
        sqlx::query(
            "INSERT INTO checkpoints (origin_swhid)
             SELECT * FROM UNNEST($1::text[])
             ON CONFLICT (origin_swhid) DO NOTHING",
        )
        .bind(&chunk_vec)
        .execute(pool)
        .await
        .context("Failed to insert checkpoints")?;
    }

    Ok(())
}

pub async fn load_checkpoints(pool: &PgPool) -> Result<HashSet<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT origin_swhid FROM checkpoints")
            .fetch_all(pool)
            .await
            .context("Failed to load checkpoints")?;

    Ok(rows.into_iter().map(|(s,)| s).collect())
}

pub async fn load_detected_commits(
    pool: &PgPool,
) -> Result<HashSet<(String, String, String, String, String)>> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT origin, snapshot_src, branch_name, missing_commit, snapshot_dst
         FROM altered_histories WHERE status = 'detected'",
    )
    .fetch_all(pool)
    .await
    .context("Failed to load detected commits")?;

    Ok(rows.into_iter().collect())
}

pub async fn promote_to_root_cause(
    pool: &PgPool,
    root_cause_commits: &HashSet<(String, String, String, String, String)>,
) -> Result<()> {
    let missing_commits: Vec<&str> = root_cause_commits
        .iter()
        .map(|r| r.3.as_str())
        .collect();

    for chunk in missing_commits.chunks(1000) {
        let chunk_vec: Vec<&str> = chunk.to_vec();
        sqlx::query(
            "UPDATE altered_histories SET status = 'root_cause'
             WHERE status = 'detected' AND missing_commit = ANY($1::text[])",
        )
        .bind(&chunk_vec)
        .execute(pool)
        .await
        .context("Failed to promote commits to root_cause")?;
    }

    sqlx::query("DELETE FROM altered_histories WHERE status = 'detected'")
        .execute(pool)
        .await
        .context("Failed to delete non-root-cause commits")?;

    Ok(())
}

pub async fn load_root_cause_commits(
    pool: &PgPool,
) -> Result<Vec<(String, String, String, String, String)>> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT origin, snapshot_src, branch_name, missing_commit, snapshot_dst
         FROM altered_histories WHERE status = 'root_cause'",
    )
    .fetch_all(pool)
    .await
    .context("Failed to load root_cause commits")?;

    Ok(rows)
}

pub async fn update_classification(
    pool: &PgPool,
    origin: &str,
    snapshot_src: &str,
    branch_name: &str,
    missing_commit: &str,
    snapshot_dst: &str,
    first_difference: Option<&str>,
    main_category: &str,
    sub_categories: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE altered_histories
         SET first_difference = $1, main_category = $2, sub_categories = $3, status = 'classified'
         WHERE origin = $4 AND snapshot_src = $5 AND branch_name = $6
           AND missing_commit = $7 AND snapshot_dst = $8 AND status = 'root_cause'",
    )
    .bind(first_difference)
    .bind(main_category)
    .bind(sub_categories)
    .bind(origin)
    .bind(snapshot_src)
    .bind(branch_name)
    .bind(missing_commit)
    .bind(snapshot_dst)
    .execute(pool)
    .await
    .context("Failed to update classification")?;

    Ok(())
}

pub async fn batch_update_classification(
    pool: &PgPool,
    updates: &[(String, String, String, String, String, Option<String>, String, String)],
) -> Result<()> {
    if updates.is_empty() {
        return Ok(());
    }

    for chunk in updates.chunks(500) {
        let mut tx = pool.begin().await?;
        for (origin, snapshot_src, branch_name, missing_commit, snapshot_dst, first_diff, main_cat, sub_cats) in chunk {
            sqlx::query(
                "UPDATE altered_histories
                 SET first_difference = $1, main_category = $2, sub_categories = $3, status = 'classified'
                 WHERE origin = $4 AND snapshot_src = $5 AND branch_name = $6
                   AND missing_commit = $7 AND snapshot_dst = $8 AND status = 'root_cause'",
            )
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

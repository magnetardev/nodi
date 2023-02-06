mod file;

use anyhow::anyhow;
use anyhow::Result;
use clap::Parser;
use file::MarkdownFile;
use sqlx::Executor;
use sqlx::Row;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use walkdir::{DirEntry, WalkDir};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    path: PathBuf,
}

fn entry_is_md_file(result: Result<DirEntry, walkdir::Error>) -> Option<DirEntry> {
    let entry = result.ok()?;
    if entry.file_type().is_file() && entry.file_name().to_str()?.ends_with(".md") {
        Some(entry)
    } else {
        None
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let path = args.path;
    if !path.exists() || !path.is_dir() {
        return Err(anyhow!(
            "expected a path to an existing directory, recieved {path:?}"
        ));
    }

    let mut config_dir = path.clone();
    config_dir.push(format!("./.{}", env!("CARGO_CRATE_NAME")));
    let mut db_path = config_dir.clone();
    db_path.push("./md.sqlite");

    let fresh_db = !db_path.exists();
    if fresh_db {
        if !config_dir.exists() {
            fs::create_dir(config_dir).await?;
        }
        let mut file = fs::File::create(&db_path).await?;
        file.flush().await?;
    }

    let pool = SqlitePool::connect(db_path.to_str().expect("unable to get path to db")).await?;
    let mut conn = pool.acquire().await?;
    if fresh_db {
        conn.execute(include_str!("../migrations/init.sql")).await?;
    }

    // Cleanup table
    // FIXME: This shouldn't need to happen, it should update in-place
    conn.execute("DELETE FROM Files").await?;
    conn.execute("DELETE FROM Relationship").await?;

    // first run, get files and their references
    let mut files_with_references: HashMap<i64, MarkdownFile> = HashMap::new();
    for entry in WalkDir::new(path).into_iter().filter_map(entry_is_md_file) {
        let file = MarkdownFile::new(entry.path().to_path_buf()).await?;
        if let Some(path) = &file.path.to_str() {
            let id = sqlx::query(
                r#"
                INSERT INTO Files (path, md5)
                VALUES (?1, ?2)
                "#,
            )
            .bind(path)
            .bind(&file.hash)
            .execute(&mut conn)
            .await?
            .last_insert_rowid();

            if !file.references.is_empty() {
                files_with_references.insert(id, file);
            }
        }
    }

    // second run, build references
    for (src, file) in files_with_references {
        for reference in file.references {
            let dst = sqlx::query(
                r#"
                SELECT (id) FROM Files WHERE Files.path LIKE '%' || ? || '.md';
                "#,
            )
            .bind(reference)
            .fetch_one(&mut conn)
            .await?
            .try_get::<i64, _>("id")?;

            sqlx::query(
                r#"
                INSERT INTO Relationship (src, dst)
                VALUES (?1, ?2)
               "#,
            )
            .bind(src)
            .bind(dst)
            .execute(&mut conn)
            .await?;
        }
    }

    Ok(())
}

mod file;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use file::{write_reference_to_db, MarkdownFile};
use sqlx::{Executor, SqlitePool};
use std::{collections::HashMap, path::PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use walkdir::{DirEntry, WalkDir};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(arg_required_else_help = true)]
    #[command(about = "Generates an index over the markdown files at the specified path", long_about = None)]
    Index { path: PathBuf },
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

    match args.command {
        Commands::Index { path } => {
            // Ensure path is valid
            if !path.exists() || !path.is_dir() {
                return Err(anyhow!(
                    "expected a path to an existing directory, recieved {path:?}"
                ));
            }

            // Get config directory
            let mut config_dir = path.clone();
            config_dir.push("./.nodi");
            let mut db_path = config_dir.clone();
            db_path.push("./index.sqlite");

            // Setup the database, if it doesn't exist
            let setup_db = !db_path.exists();
            if setup_db {
                if !config_dir.exists() {
                    fs::create_dir(config_dir).await?;
                }
                let mut file = fs::File::create(&db_path).await?;
                file.flush().await?;
            }

            // Get a connection
            let pool =
                SqlitePool::connect(db_path.to_str().expect("unable to get path to db")).await?;
            let mut conn = pool.acquire().await?;
            if setup_db {
                conn.execute(include_str!("../../../migrations/init.sql"))
                    .await?;
            }

            // Cleanup table
            // FIXME: This shouldn't need to happen, it should update in-place
            conn.execute("DELETE FROM Relationship").await?;
            conn.execute("DELETE FROM Files").await?;

            // First run, get files and their references
            let mut referencing_files: HashMap<i64, MarkdownFile> = HashMap::new();
            for entry in WalkDir::new(path).into_iter().filter_map(entry_is_md_file) {
                let file = MarkdownFile::new(entry.path().to_path_buf()).await?;
                let id = file.write_to_db(&mut conn).await?;
                referencing_files.insert(id, file);
            }

            // Second run, build references
            for (src, file) in referencing_files {
                for reference in file.references {
                    let dst = MarkdownFile::id_from_name(reference, &mut conn).await?;
                    write_reference_to_db(src, dst, &mut conn).await?;
                }
            }
        }
    }
    Ok(())
}

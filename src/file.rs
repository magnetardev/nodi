use anyhow::{anyhow, Result};
use md5_rs::{Context, DIGEST_LEN, INPUT_BUFFER_LEN};
use sqlx::{pool::PoolConnection, Row, Sqlite};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug)]
pub struct MarkdownFile {
    pub path: PathBuf,
    pub hash: String,
    pub references: Vec<String>,
}

fn hash_to_string(bytes: [u8; DIGEST_LEN]) -> String {
    bytes
        .iter()
        .map(|x| format!("{:02x}", x))
        .collect::<String>()
}

const ASCII_OPEN_CHAR: u8 = 91; // '['
const ASCII_CLOSE_CHAR: u8 = 93; // ']'

impl MarkdownFile {
    pub async fn new(path: PathBuf) -> Result<Self> {
        let mut file = tokio::fs::File::open(&path).await?;
        let mut hasher = Context::new();
        let mut reader = [0u8; INPUT_BUFFER_LEN];
        let mut references = vec![];

        // loop over the bytes
        let mut tmp_reference: Vec<u8> = vec![];
        let mut open_char_count = 0usize;
        let mut close_char_count = 0usize;
        loop {
            let length = file.read(&mut reader).await?;
            if length == 0 {
                break;
            }

            let mut start_idx: Option<usize> = None;
            let bytes = &reader[0..length];

            // FIXME: This should probably handle UTF-8...
            for (i, byte) in bytes.iter().enumerate() {
                if *byte == ASCII_OPEN_CHAR {
                    open_char_count += 1;
                    if open_char_count == 2 {
                        start_idx = Some(i);
                    }
                }

                if *byte == ASCII_CLOSE_CHAR {
                    close_char_count += 1;
                    if close_char_count == 2 {
                        let slice = &bytes[start_idx.unwrap_or(0)..i];
                        tmp_reference.extend_from_slice(slice);
                        tmp_reference.pop();
                        let contents = tmp_reference.drain(0..).skip(1).collect::<Vec<u8>>();
                        let reference = String::from_utf8(contents)?;
                        references.push(reference);
                        open_char_count = 0;
                        close_char_count = 0;
                        start_idx = None;
                    }
                }
            }

            if let Some(i) = start_idx {
                tmp_reference.extend_from_slice(&bytes[i..]);
            }

            hasher.read(bytes);
        }

        // finalize the hash
        let hash_bytes = hasher.finish();
        let hash = hash_to_string(hash_bytes);
        file.flush().await?;

        Ok(Self {
            path,
            hash,
            references,
        })
    }

    pub async fn write_to_db(&self, conn: &mut PoolConnection<Sqlite>) -> Result<i64> {
        if let Some(path) = self.path.to_str() {
            Ok(sqlx::query(
                r#"
                INSERT INTO Files (path, md5)
                VALUES (?1, ?2)
                "#,
            )
            .bind(path)
            .bind(&self.hash)
            .execute(conn)
            .await?
            .last_insert_rowid())
        } else {
            Err(anyhow!("invalid path"))
        }
    }

    pub async fn id_from_name(name: String, conn: &mut PoolConnection<Sqlite>) -> Result<i64> {
        Ok(sqlx::query(
            r#"
                SELECT (id) FROM Files WHERE Files.path LIKE '%/' || ? || '.md';
                "#,
        )
        .bind(name)
        .fetch_one(conn)
        .await?
        .try_get::<i64, _>("id")?)
    }
}

pub async fn write_reference_to_db(
    self_id: i64,
    target_id: i64,
    conn: &mut PoolConnection<Sqlite>,
) -> Result<()> {
    sqlx::query(
        r#"
                INSERT INTO Relationship (src, dst)
                VALUES (?1, ?2)
               "#,
    )
    .bind(self_id)
    .bind(target_id)
    .execute(conn)
    .await?;
    Ok(())
}

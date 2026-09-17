use crate::error::Result;
use rusqlite::{Connection, params};
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

pub struct Record {
    pub id: String,
    pub title: String,
    pub content: String,
}

pub fn search_terms(text: &str) -> Vec<String> {
    let normalized: String = text.to_lowercase().nfkc().collect();
    let expression = regex::Regex::new(r"[\p{L}\p{N}]+").expect("valid Unicode expression");
    let mut seen = HashSet::new();
    expression
        .find_iter(&normalized)
        .map(|item| item.as_str().to_owned())
        .filter(|term| seen.insert(term.clone()))
        .collect()
}

pub fn rank_lexically(records: &[Record], query: &str, limit: usize) -> Result<Vec<String>> {
    let database = Connection::open_in_memory()?;
    database.execute_batch("PRAGMA page_size=4096; PRAGMA max_page_count=32768; CREATE VIRTUAL TABLE sources USING fts5(id UNINDEXED, title, content, tokenize=unicode61)")?;
    {
        let transaction = database.unchecked_transaction()?;
        {
            let mut insert = transaction
                .prepare("INSERT INTO sources(rowid,id,title,content) VALUES(?,?,?,?)")?;
            for (index, record) in records.iter().enumerate() {
                insert.execute(params![index + 1, record.id, record.title, record.content])?;
            }
        }
        transaction.commit()?;
    }
    let expression = search_terms(query)
        .iter()
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    if expression.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = database.prepare(
        "SELECT id FROM sources WHERE sources MATCH ? ORDER BY bm25(sources), id LIMIT ?",
    )?;
    Ok(statement
        .query_map(params![expression, limit], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

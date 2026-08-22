//! Per-channel Markdown wiki pages with revision history.
//!
//! Pages are identified by a slug that is unique per channel. Every saved
//! version (including the current one) is mirrored into `wiki_revisions`;
//! old revisions are pruned on update. Concurrent edits are detected with a
//! compare-and-swap on the `revision` counter. All queries run on the single
//! connection thread, so check-then-insert sequences inside one `call_db`
//! closure are race-free.

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

use super::messages::fts_match_expression;
use super::{Db, DbCall, DbError, NOW_UTC};

/// Page metadata as carried in `wiki-index` snapshots (no body).
#[derive(Clone)]
pub struct WikiPageMeta {
    pub slug: String,
    pub title: String,
    pub revision: i64,
    pub updated_by: String,
    pub updated_at: String,
}

/// A full wiki page including its Markdown body.
#[derive(Clone)]
pub struct WikiPage {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub author: String,
    pub updated_by: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Outcome of [`create_wiki_page`].
pub enum CreateWikiResult {
    Created,
    SlugTaken,
    LimitReached,
}

/// Outcome of [`update_wiki_page`].
pub enum UpdateWikiResult {
    /// Saved; carries the new revision number.
    Saved(i64),
    /// The expected revision was stale; carries the current page so the
    /// client can show what changed.
    Conflict(WikiPage),
    NotFound,
}

/// Metadata of one stored revision, as carried in `wiki-revisions` replies.
/// The body is left out: a history list of 50 revisions of a 100 kB page
/// would otherwise ship 5 MB to draw a sidebar.
#[derive(Clone)]
pub struct WikiRevisionMeta {
    pub revision: i64,
    pub title: String,
    pub author: String,
    pub created_at: String,
    /// Size of the stored body in bytes, so the list can show what a
    /// revision changed in bulk without fetching it.
    pub bytes: i64,
}

/// One stored revision including its Markdown body.
#[derive(Clone)]
pub struct WikiRevision {
    pub revision: i64,
    pub title: String,
    pub body: String,
    pub author: String,
    pub created_at: String,
}

/// Outcome of [`restore_wiki_revision`].
pub enum RestoreWikiResult {
    /// Restored; carries the new revision number.
    Saved(i64),
    /// The expected revision was stale; carries the current page.
    Conflict(WikiPage),
    /// The page itself is gone.
    NotFound,
    /// The page exists but that revision has been pruned or never existed.
    RevisionNotFound,
}

/// A page matched by [`search_wiki_pages`], carrying an excerpt of the body
/// around the match instead of the whole document.
#[derive(Clone)]
pub struct WikiSearchHit {
    pub slug: String,
    pub title: String,
    pub snippet: String,
    pub updated_by: String,
    pub updated_at: String,
}

/// Outcome of [`rename_wiki_page`].
pub enum RenameWikiResult {
    Renamed,
    SlugTaken,
    NotFound,
}

/// Create the wiki tables, full-text index and sync triggers.
/// Called from [`super::run_schema`].
pub(super) fn wiki_schema() -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS wiki_pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels(id),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    author TEXT NOT NULL DEFAULT '',
    updated_by TEXT NOT NULL DEFAULT '',
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC}),
    updated_at TEXT NOT NULL DEFAULT ({NOW_UTC}),
    UNIQUE (channel_id, slug)
);
CREATE INDEX IF NOT EXISTS idx_wiki_pages_channel ON wiki_pages (channel_id);
CREATE TABLE IF NOT EXISTS wiki_revisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id INTEGER NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    author TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT ({NOW_UTC}),
    UNIQUE (page_id, revision)
);
CREATE VIRTUAL TABLE IF NOT EXISTS wiki_fts USING fts5(title, body);
CREATE TRIGGER IF NOT EXISTS wiki_fts_insert AFTER INSERT ON wiki_pages BEGIN
    INSERT INTO wiki_fts (rowid, title, body) VALUES (new.id, new.title, new.body);
END;
CREATE TRIGGER IF NOT EXISTS wiki_fts_update AFTER UPDATE OF title, body ON wiki_pages BEGIN
    UPDATE wiki_fts SET title = new.title, body = new.body WHERE rowid = new.id;
END;
CREATE TRIGGER IF NOT EXISTS wiki_fts_delete AFTER DELETE ON wiki_pages BEGIN
    DELETE FROM wiki_fts WHERE rowid = old.id;
END;
INSERT INTO wiki_fts (rowid, title, body)
SELECT id, title, body FROM wiki_pages WHERE id NOT IN (SELECT rowid FROM wiki_fts);
"#
    )
}

fn row_to_page(row: &rusqlite::Row<'_>) -> rusqlite::Result<WikiPage> {
    Ok(WikiPage {
        slug: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        author: row.get(3)?,
        updated_by: row.get(4)?,
        revision: row.get(5)?,
        created_at: row.get::<_, DateTime<Utc>>(6)?.to_rfc3339(),
        updated_at: row.get::<_, DateTime<Utc>>(7)?.to_rfc3339(),
    })
}

const PAGE_COLUMNS: &str =
    "slug, title, body, author, updated_by, revision, created_at, updated_at";

/// List the page metadata of a channel, ordered by title.
pub async fn list_wiki_pages(db: &Db, channel_id: i32) -> Result<Vec<WikiPageMeta>, DbError> {
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT slug, title, revision, updated_by, updated_at FROM wiki_pages \
             WHERE channel_id = ?1 ORDER BY title COLLATE NOCASE, slug",
        )?;
        let rows = stmt
            .query_map(params![channel_id], |row| {
                Ok(WikiPageMeta {
                    slug: row.get(0)?,
                    title: row.get(1)?,
                    revision: row.get(2)?,
                    updated_by: row.get(3)?,
                    updated_at: row.get::<_, DateTime<Utc>>(4)?.to_rfc3339(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// Load a full page, or `None` when the slug does not exist in the channel.
pub async fn get_wiki_page(
    db: &Db,
    channel_id: i32,
    slug: &str,
) -> Result<Option<WikiPage>, DbError> {
    let slug = slug.to_owned();
    db.call_db(move |conn| {
        conn.query_row(
            &format!("SELECT {PAGE_COLUMNS} FROM wiki_pages WHERE channel_id = ?1 AND slug = ?2"),
            params![channel_id, slug],
            row_to_page,
        )
        .optional()
    })
    .await
}

/// Create a page with revision 1 and its first revision row.
pub async fn create_wiki_page(
    db: &Db,
    channel_id: i32,
    slug: &str,
    title: &str,
    body: &str,
    author: &str,
    max_pages: i64,
) -> Result<CreateWikiResult, DbError> {
    let (slug, title, body, author) = (
        slug.to_owned(),
        title.to_owned(),
        body.to_owned(),
        author.to_owned(),
    );
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let taken = tx
            .query_row(
                "SELECT 1 FROM wiki_pages WHERE channel_id = ?1 AND slug = ?2",
                params![channel_id, slug],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if taken {
            return Ok(CreateWikiResult::SlugTaken);
        }
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM wiki_pages WHERE channel_id = ?1",
            params![channel_id],
            |row| row.get(0),
        )?;
        if count >= max_pages {
            return Ok(CreateWikiResult::LimitReached);
        }
        tx.execute(
            "INSERT INTO wiki_pages (channel_id, slug, title, body, author, updated_by) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![channel_id, slug, title, body, author],
        )?;
        let page_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO wiki_revisions (page_id, revision, title, body, author) \
             VALUES (?1, 1, ?2, ?3, ?4)",
            params![page_id, title, body, author],
        )?;
        tx.commit()?;
        Ok(CreateWikiResult::Created)
    })
    .await
}

/// Apply a new version of a page inside an open transaction: the
/// compare-and-swap on `revision`, the mirror into `wiki_revisions` and the
/// pruning of anything older than `max_revisions`. Shared by
/// [`update_wiki_page`] and [`restore_wiki_revision`], so a restore is
/// stored exactly like any other edit — history is only ever appended to,
/// never rewound.
#[allow(clippy::too_many_arguments)]
fn apply_wiki_update(
    tx: &rusqlite::Transaction<'_>,
    channel_id: i32,
    slug: &str,
    title: &str,
    body: &str,
    editor: &str,
    expected_revision: i64,
    max_revisions: i64,
) -> rusqlite::Result<UpdateWikiResult> {
    let changed = tx.execute(
        &format!(
            "UPDATE wiki_pages SET title = ?1, body = ?2, updated_by = ?3, \
             updated_at = {NOW_UTC}, revision = revision + 1 \
             WHERE channel_id = ?4 AND slug = ?5 AND revision = ?6"
        ),
        params![title, body, editor, channel_id, slug, expected_revision],
    )?;
    if changed == 0 {
        // Stale revision or missing page — report which.
        let current = tx
            .query_row(
                &format!(
                    "SELECT {PAGE_COLUMNS} FROM wiki_pages \
                     WHERE channel_id = ?1 AND slug = ?2"
                ),
                params![channel_id, slug],
                row_to_page,
            )
            .optional()?;
        return Ok(match current {
            Some(page) => UpdateWikiResult::Conflict(page),
            None => UpdateWikiResult::NotFound,
        });
    }
    let (page_id, new_revision): (i64, i64) = tx.query_row(
        "SELECT id, revision FROM wiki_pages WHERE channel_id = ?1 AND slug = ?2",
        params![channel_id, slug],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    tx.execute(
        "INSERT INTO wiki_revisions (page_id, revision, title, body, author) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![page_id, new_revision, title, body, editor],
    )?;
    tx.execute(
        "DELETE FROM wiki_revisions WHERE page_id = ?1 AND revision <= ?2",
        params![page_id, new_revision - max_revisions],
    )?;
    Ok(UpdateWikiResult::Saved(new_revision))
}

/// Save a new version of a page. The `expected_revision` compare-and-swap
/// detects concurrent edits: when it fails, the caller receives the current
/// page so the losing editor can be warned instead of silently overwritten.
#[allow(clippy::too_many_arguments)]
pub async fn update_wiki_page(
    db: &Db,
    channel_id: i32,
    slug: &str,
    title: &str,
    body: &str,
    editor: &str,
    expected_revision: i64,
    max_revisions: i64,
) -> Result<UpdateWikiResult, DbError> {
    let (slug, title, body, editor) = (
        slug.to_owned(),
        title.to_owned(),
        body.to_owned(),
        editor.to_owned(),
    );
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let result = apply_wiki_update(
            &tx,
            channel_id,
            &slug,
            &title,
            &body,
            &editor,
            expected_revision,
            max_revisions,
        )?;
        tx.commit()?;
        Ok(result)
    })
    .await
}

/// List the stored revisions of a page, newest first. Bodies are left out;
/// [`get_wiki_revision`] fetches the one the reader actually opens.
/// An unknown page yields an empty list — the same answer as a page whose
/// history has been pruned away.
pub async fn list_wiki_revisions(
    db: &Db,
    channel_id: i32,
    slug: &str,
    limit: i64,
) -> Result<Vec<WikiRevisionMeta>, DbError> {
    let slug = slug.to_owned();
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            // LENGTH() counts characters on TEXT, so the body is cast to a
            // BLOB to report the byte size the editor's cap is measured in.
            "SELECT r.revision, r.title, r.author, r.created_at, \
             LENGTH(CAST(r.body AS BLOB)) \
             FROM wiki_revisions r JOIN wiki_pages p ON p.id = r.page_id \
             WHERE p.channel_id = ?1 AND p.slug = ?2 \
             ORDER BY r.revision DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![channel_id, slug, limit], |row| {
                Ok(WikiRevisionMeta {
                    revision: row.get(0)?,
                    title: row.get(1)?,
                    author: row.get(2)?,
                    created_at: row.get::<_, DateTime<Utc>>(3)?.to_rfc3339(),
                    bytes: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

/// Load one stored revision of a page, or `None` when it was pruned, never
/// existed, or belongs to another channel.
pub async fn get_wiki_revision(
    db: &Db,
    channel_id: i32,
    slug: &str,
    revision: i64,
) -> Result<Option<WikiRevision>, DbError> {
    let slug = slug.to_owned();
    db.call_db(move |conn| {
        conn.query_row(
            "SELECT r.revision, r.title, r.body, r.author, r.created_at \
             FROM wiki_revisions r JOIN wiki_pages p ON p.id = r.page_id \
             WHERE p.channel_id = ?1 AND p.slug = ?2 AND r.revision = ?3",
            params![channel_id, slug, revision],
            |row| {
                Ok(WikiRevision {
                    revision: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                    author: row.get(3)?,
                    created_at: row.get::<_, DateTime<Utc>>(4)?.to_rfc3339(),
                })
            },
        )
        .optional()
    })
    .await
}

/// Restore an old revision as the page's newest version. The old title and
/// body are read and re-applied inside one transaction, so a concurrent save
/// either loses the compare-and-swap or is restored over — never half of
/// both. Restoring never deletes history: the old version is re-saved as a
/// *new* revision, which is what makes a restore itself undoable.
#[allow(clippy::too_many_arguments)]
pub async fn restore_wiki_revision(
    db: &Db,
    channel_id: i32,
    slug: &str,
    revision: i64,
    editor: &str,
    expected_revision: i64,
    max_revisions: i64,
) -> Result<RestoreWikiResult, DbError> {
    let (slug, editor) = (slug.to_owned(), editor.to_owned());
    db.call_db(move |conn| {
        let tx = conn.transaction()?;
        let source = tx
            .query_row(
                "SELECT r.title, r.body FROM wiki_revisions r \
                 JOIN wiki_pages p ON p.id = r.page_id \
                 WHERE p.channel_id = ?1 AND p.slug = ?2 AND r.revision = ?3",
                params![channel_id, slug, revision],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((title, body)) = source else {
            // Tell a pruned revision apart from a deleted page: only the
            // latter should send the reader back to the page list.
            let page_exists = tx
                .query_row(
                    "SELECT 1 FROM wiki_pages WHERE channel_id = ?1 AND slug = ?2",
                    params![channel_id, slug],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            return Ok(if page_exists {
                RestoreWikiResult::RevisionNotFound
            } else {
                RestoreWikiResult::NotFound
            });
        };
        let result = apply_wiki_update(
            &tx,
            channel_id,
            &slug,
            &title,
            &body,
            &editor,
            expected_revision,
            max_revisions,
        )?;
        tx.commit()?;
        Ok(match result {
            UpdateWikiResult::Saved(new_revision) => RestoreWikiResult::Saved(new_revision),
            UpdateWikiResult::Conflict(page) => RestoreWikiResult::Conflict(page),
            UpdateWikiResult::NotFound => RestoreWikiResult::NotFound,
        })
    })
    .await
}

/// Delete a page (revisions cascade, the FTS row is removed by trigger).
/// Returns `false` when the page did not exist.
pub async fn delete_wiki_page(db: &Db, channel_id: i32, slug: &str) -> Result<bool, DbError> {
    let slug = slug.to_owned();
    db.call_db(move |conn| {
        let deleted = conn.execute(
            "DELETE FROM wiki_pages WHERE channel_id = ?1 AND slug = ?2",
            params![channel_id, slug],
        )?;
        Ok(deleted > 0)
    })
    .await
}

/// Change a page's slug. Revision counter and history are untouched;
/// inbound `[[links]]` to the old slug become create-page stubs by design.
pub async fn rename_wiki_page(
    db: &Db,
    channel_id: i32,
    slug: &str,
    new_slug: &str,
) -> Result<RenameWikiResult, DbError> {
    let (slug, new_slug) = (slug.to_owned(), new_slug.to_owned());
    db.call_db(move |conn| {
        let taken = conn
            .query_row(
                "SELECT 1 FROM wiki_pages WHERE channel_id = ?1 AND slug = ?2",
                params![channel_id, new_slug],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if taken {
            return Ok(RenameWikiResult::SlugTaken);
        }
        let changed = conn.execute(
            "UPDATE wiki_pages SET slug = ?3 WHERE channel_id = ?1 AND slug = ?2",
            params![channel_id, slug, new_slug],
        )?;
        Ok(if changed > 0 {
            RenameWikiResult::Renamed
        } else {
            RenameWikiResult::NotFound
        })
    })
    .await
}

/// Existence check for `[[channel/page]]` links. Channels are addressed by
/// name; unknown channels resolve to `false`. Results align with the input
/// order.
pub async fn resolve_wiki_links(
    db: &Db,
    pairs: Vec<(String, String)>,
) -> Result<Vec<bool>, DbError> {
    db.call_db(move |conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT 1 FROM wiki_pages w JOIN channels c ON c.id = w.channel_id \
             WHERE c.name = ?1 AND w.slug = ?2",
        )?;
        let mut results = Vec::with_capacity(pairs.len());
        for (channel_name, slug) in &pairs {
            let exists = stmt
                .query_row(params![channel_name, slug], |_| Ok(()))
                .optional()?
                .is_some();
            results.push(exists);
        }
        Ok(results)
    })
    .await
}

/// Search the wiki pages of one channel through the same FTS index the
/// triggers above keep in sync, best match first.
///
/// The query is reduced to a safe MATCH expression by the shared
/// [`fts_match_expression`], so raw FTS5 operators never reach the parser.
/// Bodies may be 100 kB, so results carry an FTS `snippet()` excerpt around
/// the hit rather than the document — a match deep in a long page still
/// shows its context without shipping the page to every searcher.
pub async fn search_wiki_pages(
    db: &Db,
    channel_id: i32,
    query: &str,
    limit: i64,
) -> Result<Vec<WikiSearchHit>, DbError> {
    let Some(match_expr) = fts_match_expression(query) else {
        return Ok(Vec::new());
    };
    db.call_db(move |conn| {
        // The FTS table is not aliased: `snippet()` addresses it by name.
        let mut stmt = conn.prepare_cached(
            "SELECT wiki_pages.slug, wiki_pages.title, \
             snippet(wiki_fts, 1, '', '', '…', 16), \
             wiki_pages.updated_by, wiki_pages.updated_at \
             FROM wiki_fts JOIN wiki_pages ON wiki_pages.id = wiki_fts.rowid \
             WHERE wiki_fts MATCH ?1 AND wiki_pages.channel_id = ?2 \
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![match_expr, channel_id, limit], |row| {
                Ok(WikiSearchHit {
                    slug: row.get(0)?,
                    title: row.get(1)?,
                    snippet: row.get(2)?,
                    updated_by: row.get(3)?,
                    updated_at: row.get::<_, DateTime<Utc>>(4)?.to_rfc3339(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
}

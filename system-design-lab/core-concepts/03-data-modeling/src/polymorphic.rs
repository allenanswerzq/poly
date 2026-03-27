use rusqlite::Connection;

// =============================================================================
// Polymorphic Associations — one table referencing multiple entity types
//
//   Problem: comments can belong to posts, photos, OR videos.
//   How to model "one comment table for many entity types"?
//
//   comments
//   ┌──────────────────┬──────────────────┬───────────────┐
//   │ commentable_type │ commentable_id   │ content       │
//   ├──────────────────┼──────────────────┼───────────────┤
//   │ post             │ 1                │ "Great post!" │
//   │ post             │ 1                │ "Nice read."  │
//   │ photo            │ 1                │ "Cool pic!"   │
//   │ video            │ 1                │ "So funny!"   │
//   └──────────────────┴──────────────────┴───────────────┘
//         ↑ type column            ↑ id of that entity
//
//   Query: WHERE commentable_type = 'video' AND commentable_id = 1
//   Index: (commentable_type, commentable_id) for fast lookups
//
//   Tradeoff: simple single table, but no FK constraint on commentable_id
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Polymorphic Associations ═══\n");

    let db = Connection::open_in_memory().unwrap();

    db.execute_batch(
        "
        -- The entities
        CREATE TABLE posts  (id INTEGER PRIMARY KEY, title TEXT);
        CREATE TABLE photos (id INTEGER PRIMARY KEY, url TEXT);
        CREATE TABLE videos (id INTEGER PRIMARY KEY, title TEXT);

        -- Single comments table for ALL entity types
        CREATE TABLE comments (
            id INTEGER PRIMARY KEY,
            commentable_type TEXT NOT NULL,  -- 'post', 'photo', 'video'
            commentable_id INTEGER NOT NULL,
            author TEXT NOT NULL,
            content TEXT NOT NULL
        );
        -- Composite index for fast lookup by (type, id)
        CREATE INDEX idx_commentable ON comments(commentable_type, commentable_id);

        -- Sample data
        INSERT INTO posts VALUES (1, 'My first post');
        INSERT INTO photos VALUES (1, 'sunset.jpg');
        INSERT INTO videos VALUES (1, 'funny-cat.mp4');

        INSERT INTO comments VALUES (1, 'post',  1, 'Alice', 'Great post!');
        INSERT INTO comments VALUES (2, 'post',  1, 'Bob',   'Interesting read.');
        INSERT INTO comments VALUES (3, 'photo', 1, 'Charlie', 'Nice photo!');
        INSERT INTO comments VALUES (4, 'video', 1, 'Alice', 'Love this video.');
        INSERT INTO comments VALUES (5, 'video', 1, 'Bob',   'So funny!');
        INSERT INTO comments VALUES (6, 'video', 1, 'Charlie', 'lol');
    ",
    )
    .unwrap();

    println!("    Same comments table handles posts, photos, and videos:\n");

    for (entity_type, label) in &[("post", "Post"), ("photo", "Photo"), ("video", "Video")] {
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM comments WHERE commentable_type = ?1 AND commentable_id = 1",
                [entity_type],
                |r| r.get(0),
            )
            .unwrap();

        println!("    {} comments ({}):", label, count);

        let mut stmt = db
            .prepare(
                "SELECT author, content FROM comments
             WHERE commentable_type = ?1 AND commentable_id = 1",
            )
            .unwrap();
        let rows: Vec<String> = stmt
            .query_map([entity_type], |row| {
                Ok(format!(
                    "      {}: \"{}\"",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?
                ))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        for row in &rows {
            println!("{}", row);
        }
    }

    // Show the EXPLAIN QUERY PLAN to prove index is used
    println!("\n    Query plan (uses the composite index):");
    let _plan: String = db
        .query_row(
            "EXPLAIN QUERY PLAN SELECT * FROM comments
         WHERE commentable_type = 'video' AND commentable_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "unknown".into());
    // Get all plan rows
    let mut stmt = db
        .prepare(
            "EXPLAIN QUERY PLAN SELECT * FROM comments
         WHERE commentable_type = 'video' AND commentable_id = 1",
        )
        .unwrap();
    let plans: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    for p in &plans {
        println!("    {}", p);
    }

    println!("\n    Schema: comments(commentable_type, commentable_id, content)");
    println!("    Index on (commentable_type, commentable_id) for fast lookups.");
    println!("    Tradeoff: simple but no FK constraint on commentable_id.\n");
}

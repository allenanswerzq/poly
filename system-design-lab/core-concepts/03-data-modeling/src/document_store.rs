use bson::{doc, Document};
use polodb_core::CollectionT;
use polodb_core::Database;

// =============================================================================
// Document Store — using PoloDB (real MongoDB-compatible embedded document DB)
//
//   products collection:
//   ┌────────────────────────────────────────────┐
//   │ { "name": "Widget",                       │
//   │   "price": 9.99,                          │
//   │   "tags": ["electronics"],                │
//   │   "specs": {"weight": "200g"} }           │  ← document 1
//   ├────────────────────────────────────────────┤
//   │ { "name": "Gadget",                       │
//   │   "price": 24.99,                         │
//   │   "specs": {"battery": "lithium"} }       │  ← different fields!
//   ├────────────────────────────────────────────┤
//   │ { "name": "Doohickey",                    │
//   │   "reviews": [{"user":"Alice","rating":5}]│  ← embedded array
//   │ }                                          │
//   └────────────────────────────────────────────┘
//
//   This is PoloDB — a real document database with MongoDB-like API.
//   Not SQLite with JSON. Actual BSON documents, actual MongoDB queries.
//
//   Pros: flexible schema, nested data, no JOINs for related data
//   Cons: no referential integrity, no JOINs, data duplication
// =============================================================================

pub fn demo() {
    println!("\n  ═══ Document Store (PoloDB — MongoDB-like) ═══\n");
    println!("    Using PoloDB: a real embedded document database (MongoDB API).\n");

    // Open PoloDB in a temp directory (it's a real on-disk document DB)
    let tmp_dir = std::env::temp_dir().join("polodb_demo");
    let _ = std::fs::remove_dir_all(&tmp_dir); // clean from previous runs
    let db = Database::open_path(&tmp_dir).unwrap();
    let products = db.collection::<Document>("products");

    // Insert documents with DIFFERENT schemas — this is the point of a doc store
    products
        .insert_one(doc! {
            "name": "Widget",
            "price": 9.99,
            "tags": ["electronics", "gadgets"],
            "specs": { "weight": "200g", "color": "blue" }
        })
        .unwrap();

    products
        .insert_one(doc! {
            "name": "Gadget",
            "price": 24.99,
            "tags": ["electronics"],
            "specs": { "weight": "500g", "color": "red", "battery": "lithium" }
            // ↑ extra field 'battery' — no schema migration needed
        })
        .unwrap();

    // Embed related data directly (like MongoDB subdocuments)
    products
        .insert_one(doc! {
            "name": "Doohickey",
            "price": 4.99,
            "reviews": [
                { "user": "Alice", "rating": 5, "text": "Love it!" },
                { "user": "Bob", "rating": 3, "text": "It's okay" }
            ]
            // ↑ reviews embedded in the document, no separate table needed
        })
        .unwrap();

    // Read a document with embedded reviews — no JOINs
    println!("    db.collection('products').find({{name: 'Doohickey'}})\n");
    let result = products.find(doc! { "name": "Doohickey" }).run().unwrap();
    for doc in result {
        let doc = doc.unwrap();
        println!("    {}\n", doc);
    }

    // Query: flexible schema — different docs have different fields
    println!("    Flexible schema — query docs with/without 'battery' field:");
    let all: Vec<_> = products
        .find(doc! {})
        .run()
        .unwrap()
        .filter_map(|d| d.ok())
        .collect();
    for d in &all {
        let name = d.get_str("name").unwrap_or("?");
        let battery = d
            .get_document("specs")
            .ok()
            .and_then(|s| s.get_str("battery").ok());
        println!("    {}: battery = {:?}", name, battery);
    }

    // Query by field value — like MongoDB find()
    println!("\n    db.collection('products').find({{price: {{'$gt': 10}}}})");
    let expensive: Vec<_> = products
        .find(doc! {
            "price": { "$gt": 10.0 }
        })
        .run()
        .unwrap()
        .filter_map(|d| d.ok())
        .collect();
    println!("    Found {} products with price > $10:", expensive.len());
    for d in &expensive {
        println!(
            "    → {} (${:.2})",
            d.get_str("name").unwrap_or("?"),
            d.get_f64("price").unwrap_or(0.0)
        );
    }

    // Count documents
    let total = products.count_documents().unwrap();
    println!("\n    Total documents in collection: {}", total);

    println!("\n    PoloDB: real document DB, MongoDB-compatible API.");
    println!("    BSON documents, flexible schema, embedded data, no JOINs.");
    println!("    In production: MongoDB Atlas, DocumentDB, CosmosDB.\n");
}

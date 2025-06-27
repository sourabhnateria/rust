#[cfg(target_os = "windows")]
#[link(name = "advapi32")]
// extern "C" {}
use heed::types::*;
use heed::{Database, EnvOpenOptions};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for the database
    let dir = tempdir()?;
    println!(
        "✅ Temporary database directory created at: {:?}",
        dir.path()
    );

    // Open environment (unsafe required)
    let env = unsafe { EnvOpenOptions::new().open(dir.path())? };
    println!("✅ Heed environment opened");

    // Start a write transaction and create unnamed default database
    let mut wtxn = env.write_txn()?;
    let db: Database<Str, U32<byteorder::NativeEndian>> = env.create_database(&mut wtxn, None)?;
    println!("✅ Database created");

    // ---------- INSERT (CREATE) ----------
    db.put(&mut wtxn, "seven", &7)?;
    db.put(&mut wtxn, "zero", &0)?;
    db.put(&mut wtxn, "five", &5)?;
    db.put(&mut wtxn, "three", &3)?;
    wtxn.commit()?;
    println!("✅ Data inserted");

    // ---------- READ ----------
    let rtxn = env.read_txn()?;

    let keys = ["zero", "five", "seven", "three"];
    for key in &keys {
        let ret = db.get(&rtxn, key)?;
        println!("📖 Read {}: {:?}", key, ret);
    }

    drop(rtxn); // Close read transaction

    // ---------- UPDATE ----------
    let mut wtxn = env.write_txn()?;
    db.put(&mut wtxn, "three", &33)?;
    wtxn.commit()?;
    println!("✏️ Updated key 'three' to 33");

    let rtxn = env.read_txn()?;
    let updated = db.get(&rtxn, "three")?;
    println!("📖 Read updated 'three': {:?}", updated);

    drop(rtxn);

    // ---------- DELETE ----------
    let mut wtxn = env.write_txn()?;
    db.delete(&mut wtxn, "five")?;
    wtxn.commit()?;
    println!("🗑️ Deleted key 'five'");

    let rtxn = env.read_txn()?;
    let deleted = db.get(&rtxn, "five")?;
    println!("📖 Check 'five' after deletion: {:?}", deleted);

    Ok(())
}

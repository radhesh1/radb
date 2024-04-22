//! radump is a debug tool that prints a raDB BitCask database in
//! human-readable form. It only prints live BitCask data, not garbage entries.
#![warn(clippy::all)]

use radb::error::{Error, Result};
use radb::storage::debug;
use radb::storage::{BitCask, Engine};

fn main() -> Result<()> {
    let args = clap::command!()
        .about("Prints raDB BitCask contents in human-readable form.")
        .args([clap::Arg::new("file").required(true)])
        .get_matches();
    let file: &String = args.get_one("file").unwrap();

    let mut engine = BitCask::new(file.into())?;
    let mut scan = engine.scan(..);
    while let Some((key, value)) = scan.next().transpose()? {
        let (fkey, Some(fvalue)) = debug::format_key_value(&key, &Some(value)) else {
            return Err(Error::Internal(format!("Unexpected missing value at key {:?}", key)));
        };
        println!("{} → {}", fkey, fvalue);
    }
    Ok(())
}

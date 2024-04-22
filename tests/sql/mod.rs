mod expression;
mod mutation;
mod query;
mod schema;

use radb::error::Result;
use radb::sql::engine::{Engine, KV};
use radb::storage;

/// Sets up a basic in-memory SQL engine with an initial dataset.
fn setup(queries: Vec<&str>) -> Result<KV<storage::Memory>> {
    let engine = KV::new(storage::Memory::new());
    let mut session = engine.session();
    session.execute("BEGIN")?;
    for query in queries {
        session.execute(query)?;
    }
    session.execute("COMMIT")?;
    Ok(engine)
}

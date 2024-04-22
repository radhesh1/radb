//! End-to-end tests for raDB. These spin up raDB clusters as separate child
//! processes using a built binary.
//!
//! TODO: these tests should be rewritten as data-driven golden master tests.

mod client;
pub mod dataset;
mod isolation;
mod recovery;
mod testcluster;

use testcluster::TestCluster;

/// Asserts that a resultset contains the expected rows.
fn assert_rows(result: radb::ResultSet, expect: Vec<radb::sql::types::Row>) {
    match result {
        radb::ResultSet::Query { rows, .. } => {
            pretty_assertions::assert_eq!(rows.collect::<Result<Vec<_>, _>>().unwrap(), expect)
        }
        r => panic!("Unexpected result {:?}", r),
    }
}

/// Asserts that a resultset contains the single expected row.
fn assert_row(result: radb::ResultSet, expect: radb::sql::types::Row) {
    assert_rows(result, vec![expect])
}

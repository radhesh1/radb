use super::{assert_row, assert_rows, dataset, TestCluster};

use radb::error::{Error, Result};
use radb::raft;
use radb::server::Status;
use radb::sql::execution::ResultSet;
use radb::sql::schema;
use radb::sql::types::{Column, DataType, Value};
use radb::storage;
use radb::storage::{engine, mvcc};

use pretty_assertions::assert_eq;
use serial_test::serial;

#[test]
#[serial]
fn get_table() -> Result<()> {
    let tc = TestCluster::run_with(5, dataset::MOVIES)?;
    let mut c = tc.connect_any()?;

    assert_eq!(c.get_table("unknown"), Err(Error::Value("Table unknown does not exist".into())));
    assert_eq!(
        c.get_table("movies")?,
        schema::Table {
            name: "movies".into(),
            columns: vec![
                schema::Column {
                    name: "id".into(),
                    datatype: DataType::Integer,
                    primary_key: true,
                    nullable: false,
                    default: None,
                    unique: true,
                    index: false,
                    references: None,
                },
                schema::Column {
                    name: "title".into(),
                    datatype: DataType::String,
                    primary_key: false,
                    nullable: false,
                    default: None,
                    unique: false,
                    index: false,
                    references: None,
                },
                schema::Column {
                    name: "studio_id".into(),
                    datatype: DataType::Integer,
                    primary_key: false,
                    nullable: false,
                    default: None,
                    unique: false,
                    index: false,
                    references: Some("studios".into()),
                },
                schema::Column {
                    name: "genre_id".into(),
                    datatype: DataType::Integer,
                    primary_key: false,
                    nullable: false,
                    default: None,
                    unique: false,
                    index: false,
                    references: Some("genres".into()),
                },
                schema::Column {
                    name: "released".into(),
                    datatype: DataType::Integer,
                    primary_key: false,
                    nullable: false,
                    default: None,
                    unique: false,
                    index: false,
                    references: None,
                },
                schema::Column {
                    name: "rating".into(),
                    datatype: DataType::Float,
                    primary_key: false,
                    nullable: true,
                    default: Some(Value::Null),
                    unique: false,
                    index: false,
                    references: None,
                },
                schema::Column {
                    name: "ultrahd".into(),
                    datatype: DataType::Boolean,
                    primary_key: false,
                    nullable: true,
                    default: Some(Value::Null),
                    unique: false,
                    index: false,
                    references: None,
                },
            ]
        }
    );
    Ok(())
}

#[test]
#[serial]
fn list_tables() -> Result<()> {
    let tc = TestCluster::run_with(5, dataset::MOVIES)?;
    let mut c = tc.connect_any()?;

    assert_eq!(c.list_tables()?, vec!["countries", "genres", "movies", "studios"]);
    Ok(())
}

#[test]
#[serial]
fn status() -> Result<()> {
    let tc = TestCluster::run_with(1, dataset::MOVIES)?;
    let mut c = tc.connect_any()?;

    assert_eq!(
        c.status()?,
        Status {
            server: 1,
            raft: raft::Status {
                leader: 1,
                term: 1,
                last_index: [(1, 27)].into(),
                commit_index: 27,
                apply_index: 27,
                storage: storage::engine::Status {
                    name: "bitcask".to_string(),
                    keys: 29,
                    size: 1322,
                    total_disk_size: 1829,
                    live_disk_size: 1554,
                    garbage_disk_size: 275
                },
            },
            mvcc: mvcc::Status {
                versions: 1,
                active_txns: 0,
                storage: engine::Status {
                    name: "bitcask".to_string(),
                    keys: 26,
                    size: 1630,
                    total_disk_size: 4556,
                    live_disk_size: 1838,
                    garbage_disk_size: 2718
                },
            }
        },
    );
    Ok(())
}

#[test]
#[serial]
fn execute() -> Result<()> {
    let tc = TestCluster::run_with(5, dataset::MOVIES)?;
    let mut c = tc.connect_any()?;

    // SELECT
    let result = c.execute("SELECT * FROM genres")?;
    assert_eq!(
        result,
        ResultSet::Query {
            columns: vec![Column { name: Some("id".into()) }, Column { name: Some("name".into()) }],
            rows: Box::new(std::iter::empty()),
        }
    );
    assert_rows(
        result,
        vec![
            vec![Value::Integer(1), Value::String("Science Fiction".into())],
            vec![Value::Integer(2), Value::String("Action".into())],
            vec![Value::Integer(3), Value::String("Comedy".into())],
        ],
    );

    let result = c.execute("SELECT * FROM genres WHERE FALSE")?;
    assert_eq!(
        result,
        ResultSet::Query {
            columns: vec![Column { name: Some("id".into()) }, Column { name: Some("name".into()) }],
            rows: Box::new(std::iter::empty()),
        }
    );
    assert_rows(result, Vec::new());

    assert_eq!(c.execute("SELECT * FROM x"), Err(Error::Value("Table x does not exist".into())));

    // INSERT
    assert_eq!(
        c.execute("INSERT INTO genres VALUES (1, 'Western')"),
        Err(Error::Value("Primary key 1 already exists for table genres".into())),
    );
    assert_eq!(
        c.execute("INSERT INTO genres VALUES (9, 'Western')"),
        Ok(ResultSet::Create { count: 1 }),
    );
    assert_eq!(
        c.execute("INSERT INTO x VALUES (9, 'Western')"),
        Err(Error::Value("Table x does not exist".into()))
    );

    // UPDATE
    assert_eq!(
        c.execute("UPDATE genres SET name = 'Horror' WHERE FALSE"),
        Ok(ResultSet::Update { count: 0 }),
    );
    assert_eq!(
        c.execute("UPDATE genres SET name = 'Horror' WHERE id = 9"),
        Ok(ResultSet::Update { count: 1 }),
    );
    assert_eq!(
        c.execute("UPDATE genres SET id = 1 WHERE id = 9"),
        Err(Error::Value("Primary key 1 already exists for table genres".into()))
    );

    // DELETE
    assert_eq!(c.execute("DELETE FROM genres WHERE FALSE"), Ok(ResultSet::Delete { count: 0 }),);
    assert_eq!(c.execute("DELETE FROM genres WHERE id = 9"), Ok(ResultSet::Delete { count: 1 }),);
    assert_eq!(
        c.execute("DELETE FROM genres WHERE x = 1"),
        Err(Error::Value("Unknown field x".into()))
    );

    Ok(())
}

#[test]
#[serial]
fn execute_txn() -> Result<()> {
    let tc = TestCluster::run_with(5, dataset::MOVIES)?;
    let mut c = tc.connect_any()?;

    assert_eq!(c.txn(), None);

    // Committing a change in a txn should work
    assert_eq!(c.execute("BEGIN")?, ResultSet::Begin { version: 2, read_only: false });
    assert_eq!(c.txn(), Some((2, false)));
    c.execute("INSERT INTO genres VALUES (4, 'Drama')")?;
    assert_eq!(c.execute("COMMIT")?, ResultSet::Commit { version: 2 });
    assert_eq!(c.txn(), None);
    assert_row(
        c.execute("SELECT * FROM genres WHERE id = 4")?,
        vec![Value::Integer(4), Value::String("Drama".into())],
    );
    assert_eq!(c.txn(), None);

    // Rolling back a change in a txn should also work
    assert_eq!(c.execute("BEGIN")?, ResultSet::Begin { version: 3, read_only: false });
    assert_eq!(c.txn(), Some((3, false)));
    c.execute("INSERT INTO genres VALUES (5, 'Musical')")?;
    assert_row(
        c.execute("SELECT * FROM genres WHERE id = 5")?,
        vec![Value::Integer(5), Value::String("Musical".into())],
    );
    assert_eq!(c.execute("ROLLBACK")?, ResultSet::Rollback { version: 3 });
    assert_rows(c.execute("SELECT * FROM genres WHERE id = 5")?, Vec::new());
    assert_eq!(c.txn(), None);

    // Starting a read-only txn should block writes
    assert_eq!(c.execute("BEGIN READ ONLY")?, ResultSet::Begin { version: 4, read_only: true });
    assert_eq!(c.txn(), Some((4, true)));
    assert_row(
        c.execute("SELECT * FROM genres WHERE id = 4")?,
        vec![Value::Integer(4), Value::String("Drama".into())],
    );
    assert_eq!(c.execute("INSERT INTO genres VALUES (5, 'Musical')"), Err(Error::ReadOnly));
    assert_row(
        c.execute("SELECT * FROM genres WHERE id = 4")?,
        vec![Value::Integer(4), Value::String("Drama".into())],
    );
    assert_eq!(c.execute("COMMIT")?, ResultSet::Commit { version: 4 });

    // Starting a time-travel txn should work, it shouldn't see recent changes, and it should
    // block writes
    assert_eq!(
        c.execute("BEGIN READ ONLY AS OF SYSTEM TIME 2")?,
        ResultSet::Begin { version: 2, read_only: true },
    );
    assert_eq!(c.txn(), Some((2, true)));
    assert_rows(
        c.execute("SELECT * FROM genres")?,
        vec![
            vec![Value::Integer(1), Value::String("Science Fiction".into())],
            vec![Value::Integer(2), Value::String("Action".into())],
            vec![Value::Integer(3), Value::String("Comedy".into())],
        ],
    );
    assert_eq!(c.execute("INSERT INTO genres VALUES (5, 'Musical')"), Err(Error::ReadOnly));
    assert_eq!(c.execute("COMMIT")?, ResultSet::Commit { version: 2 });

    // A txn should still be usable after an error occurs
    assert_eq!(c.execute("BEGIN")?, ResultSet::Begin { version: 4, read_only: false });
    c.execute("INSERT INTO genres VALUES (5, 'Horror')")?;
    assert_eq!(
        c.execute("INSERT INTO genres VALUES (5, 'Musical')"),
        Err(Error::Value("Primary key 5 already exists for table genres".into()))
    );
    assert_eq!(c.txn(), Some((4, false)));
    c.execute("INSERT INTO genres VALUES (6, 'Western')")?;
    assert_eq!(c.execute("COMMIT")?, ResultSet::Commit { version: 4 });
    assert_rows(
        c.execute("SELECT * FROM genres")?,
        vec![
            vec![Value::Integer(1), Value::String("Science Fiction".into())],
            vec![Value::Integer(2), Value::String("Action".into())],
            vec![Value::Integer(3), Value::String("Comedy".into())],
            vec![Value::Integer(4), Value::String("Drama".into())],
            vec![Value::Integer(5), Value::String("Horror".into())],
            vec![Value::Integer(6), Value::String("Western".into())],
        ],
    );

    Ok(())
}

#[test]
#[serial]
fn execute_txn_concurrent() -> Result<()> {
    let tc = TestCluster::run_with(5, dataset::MOVIES)?;
    let mut a = tc.connect_any()?;
    let mut b = tc.connect_any()?;

    // Concurrent updates should throw a serialization failure on conflict.
    assert_eq!(a.execute("BEGIN")?, ResultSet::Begin { version: 2, read_only: false });
    assert_eq!(b.execute("BEGIN")?, ResultSet::Begin { version: 3, read_only: false });

    assert_row(
        a.execute("SELECT * FROM genres WHERE id = 1")?,
        vec![Value::Integer(1), Value::String("Science Fiction".into())],
    );
    assert_row(
        b.execute("SELECT * FROM genres WHERE id = 1")?,
        vec![Value::Integer(1), Value::String("Science Fiction".into())],
    );

    assert_eq!(
        a.execute("UPDATE genres SET name = 'x' WHERE id = 1"),
        Ok(ResultSet::Update { count: 1 })
    );
    assert_eq!(b.execute("UPDATE genres SET name = 'y' WHERE id = 1"), Err(Error::Serialization));

    assert_eq!(a.execute("COMMIT"), Ok(ResultSet::Commit { version: 2 }));
    assert_eq!(b.execute("ROLLBACK"), Ok(ResultSet::Rollback { version: 3 }));

    assert_row(
        a.execute("SELECT * FROM genres WHERE id = 1")?,
        vec![Value::Integer(1), Value::String("x".into())],
    );

    Ok(())
}

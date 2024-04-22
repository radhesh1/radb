/*
 * rasql is a command-line client for raDB. It connects to a raDB cluster node and executes SQL
 * queries against it via a REPL interface.
 */

#![warn(clippy::all)]

use radb::error::{Error, Result};
use radb::sql::execution::ResultSet;
use radb::sql::parser::{Lexer, Token};
use radb::Client;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{error::ReadlineError, Editor, Modifiers};
use rustyline_derive::{Completer, Helper, Highlighter, Hinter};

fn main() -> Result<()> {
    let opts = clap::command!()
        .name("rasql")
        .about("A RaDB client.")
        .args([
            clap::Arg::new("command"),
            clap::Arg::new("host")
                .short('H')
                .long("host")
                .help("Host to connect to")
                .default_value("127.0.0.1"),
            clap::Arg::new("port")
                .short('p')
                .long("port")
                .help("Port number to connect to")
                .value_parser(clap::value_parser!(u16))
                .default_value("9605"),
        ])
        .get_matches();

    let mut rasql =
        RaSQL::new(opts.get_one::<String>("host").unwrap(), *opts.get_one("port").unwrap())?;

    if let Some(command) = opts.get_one::<&str>("command") {
        rasql.execute(command)
    } else {
        rasql.run()
    }
}

/// The RaSQL REPL
struct RaSQL {
    client: Client,
    editor: Editor<InputValidator, DefaultHistory>,
    history_path: Option<std::path::PathBuf>,
    show_headers: bool,
}

impl RaSQL {
    /// Creates a new RaSQL REPL for the given server host and port
    fn new(host: &str, port: u16) -> Result<Self> {
        Ok(Self {
            client: Client::new((host, port))?,
            editor: Editor::new()?,
            history_path: std::env::var_os("HOME")
                .map(|home| std::path::Path::new(&home).join(".rasql.history")),
            show_headers: false,
        })
    }

    /// Executes a line of input
    fn execute(&mut self, input: &str) -> Result<()> {
        if input.starts_with('!') {
            self.execute_command(input)
        } else if !input.is_empty() {
            self.execute_query(input)
        } else {
            Ok(())
        }
    }

    /// Handles a REPL command (prefixed by !, e.g. !help)
    fn execute_command(&mut self, input: &str) -> Result<()> {
        let mut input = input.split_ascii_whitespace();
        let command = input.next().ok_or_else(|| Error::Parse("Expected command.".to_string()))?;

        let getargs = |n| {
            let args: Vec<&str> = input.collect();
            if args.len() != n {
                Err(Error::Parse(format!("{}: expected {} args, got {}", command, n, args.len())))
            } else {
                Ok(args)
            }
        };

        match command {
            "!headers" => match getargs(1)?[0] {
                "on" => {
                    self.show_headers = true;
                    println!("Headers enabled");
                }
                "off" => {
                    self.show_headers = false;
                    println!("Headers disabled");
                }
                v => return Err(Error::Parse(format!("Invalid value {}, expected on or off", v))),
            },
            "!help" => println!(
                r#"
Enter a SQL statement terminated by a semicolon (;) to execute it and display the result.
The following commands are also available:

    !headers <on|off>  Enable or disable column headers
    !help              This help message
    !status            Display server status
    !table [table]     Display table schema, if it exists
    !tables            List tables
"#
            ),
            "!status" => {
                let status = self.client.status()?;
                let mut node_logs = status
                    .raft
                    .last_index
                    .iter()
                    .map(|(id, index)| format!("{}:{}", id, index))
                    .collect::<Vec<_>>();
                node_logs.sort();
                println!(
                    r#"
Server:    {server} (leader {leader} in term {term} with {nodes} nodes)
Raft log:  {committed} committed, {applied} applied, {raft_size} MB ({raft_storage} storage)
Node logs: {logs}
MVCC:      {active_txns} active txns, {versions} versions
Storage:   {keys} keys, {logical_size} MB logical, {nodes}x {disk_size} MB disk, {garbage_percent}% garbage ({sql_storage} engine)
"#,
                    server = status.server,
                    leader = status.raft.leader,
                    term = status.raft.term,
                    nodes = status.raft.last_index.len(),
                    committed = status.raft.commit_index,
                    applied = status.raft.apply_index,
                    raft_storage = status.raft.storage.name,
                    raft_size =
                        format_args!("{:.3}", status.raft.storage.size as f64 / 1000.0 / 1000.0),
                    logs = node_logs.join(" "),
                    versions = status.mvcc.versions,
                    active_txns = status.mvcc.active_txns,
                    keys = status.mvcc.storage.keys,
                    logical_size =
                        format_args!("{:.3}", status.mvcc.storage.size as f64 / 1000.0 / 1000.0),
                    garbage_percent = format_args!(
                        "{:.0}",
                        if status.mvcc.storage.total_disk_size > 0 {
                            status.mvcc.storage.garbage_disk_size as f64
                                / status.mvcc.storage.total_disk_size as f64
                                * 100.0
                        } else {
                            0.0
                        }
                    ),
                    disk_size = format_args!(
                        "{:.3}",
                        status.mvcc.storage.total_disk_size as f64 / 1000.0 / 1000.0
                    ),
                    sql_storage = status.mvcc.storage.name,
                )
            }
            "!table" => {
                let args = getargs(1)?;
                println!("{}", self.client.get_table(args[0])?);
            }
            "!tables" => {
                getargs(0)?;
                for table in self.client.list_tables()? {
                    println!("{}", table)
                }
            }
            c => return Err(Error::Parse(format!("Unknown command {}", c))),
        }
        Ok(())
    }

    /// Runs a query and displays the results
    fn execute_query(&mut self, query: &str) -> Result<()> {
        match self.client.execute(query)? {
            ResultSet::Begin { version, read_only } => match read_only {
                false => println!("Began transaction at new version {}", version),
                true => println!("Began read-only transaction at version {}", version),
            },
            ResultSet::Commit { version: id } => println!("Committed transaction {}", id),
            ResultSet::Rollback { version: id } => println!("Rolled back transaction {}", id),
            ResultSet::Create { count } => println!("Created {} rows", count),
            ResultSet::Delete { count } => println!("Deleted {} rows", count),
            ResultSet::Update { count } => println!("Updated {} rows", count),
            ResultSet::CreateTable { name } => println!("Created table {}", name),
            ResultSet::DropTable { name, existed } => match existed {
                true => println!("Dropped table {}", name),
                false => println!("Table {} did not exit", name),
            },
            ResultSet::Explain(plan) => println!("{}", plan),
            ResultSet::Query { columns, mut rows } => {
                if self.show_headers {
                    println!(
                        "{}",
                        columns
                            .iter()
                            .map(|c| c.name.as_deref().unwrap_or("?"))
                            .collect::<Vec<_>>()
                            .join("|")
                    );
                }
                while let Some(row) = rows.next().transpose()? {
                    println!(
                        "{}",
                        row.into_iter().map(|v| format!("{}", v)).collect::<Vec<_>>().join("|")
                    );
                }
            }
        }
        Ok(())
    }

    /// Prompts the user for input
    fn prompt(&mut self) -> Result<Option<String>> {
        let prompt = match self.client.txn() {
            Some((version, false)) => format!("radb:{}> ", version),
            Some((version, true)) => format!("radb@{}> ", version),
            None => "radb> ".into(),
        };
        match self.editor.readline(&prompt) {
            Ok(input) => {
                self.editor.add_history_entry(&input)?;
                Ok(Some(input.trim().to_string()))
            }
            Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Runs the RaSQL REPL
    fn run(&mut self) -> Result<()> {
        if let Some(path) = &self.history_path {
            match self.editor.load_history(path) {
                Ok(_) => {}
                Err(ReadlineError::Io(ref err)) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            };
        }
        self.editor.set_helper(Some(InputValidator));
        // Make sure multiline pastes are interpreted as normal inputs.
        self.editor.bind_sequence(
            rustyline::KeyEvent(rustyline::KeyCode::BracketedPasteStart, Modifiers::NONE),
            rustyline::Cmd::Noop,
        );

        let status = self.client.status()?;
        println!("Connected to raDB node \"{}\". Enter !help for instructions.", status.server);

        while let Some(input) = self.prompt()? {
            match self.execute(&input) {
                Ok(()) => {}
                error @ Err(Error::Internal(_)) => return error,
                Err(error) => println!("Error: {}", error),
            }
        }

        if let Some(path) = &self.history_path {
            self.editor.save_history(path)?;
        }
        Ok(())
    }
}

/// A Rustyline helper for multiline editing. It parses input lines and determines if they make up a
/// complete command or not.
#[derive(Completer, Helper, Highlighter, Hinter)]
struct InputValidator;

impl Validator for InputValidator {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

        // Empty lines and ! commands are fine.
        if input.is_empty() || input.starts_with('!') || input == ";" {
            return Ok(ValidationResult::Valid(None));
        }

        // For SQL statements, just look for any semicolon or lexer error and if found accept the
        // input and rely on the server to do further validation and error handling. Otherwise,
        // wait for more input.
        for result in Lexer::new(ctx.input()) {
            match result {
                Ok(Token::Semicolon) => return Ok(ValidationResult::Valid(None)),
                Err(_) => return Ok(ValidationResult::Valid(None)),
                _ => {}
            }
        }
        Ok(ValidationResult::Incomplete)
    }

    fn validate_while_typing(&self) -> bool {
        false
    }
}

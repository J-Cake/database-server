use std::fs::OpenOptions;
use std::io::{stderr, BufRead, BufReader, Write};
use std::path::PathBuf;
use libdb::Database;
use libdb::error::Result;

pub fn main() {
    let mut buf = String::new();

    let mut db = None;

    print_errors(|| {
        match prompt("> ") {
            cmd if cmd.starts_with("open") => {
                db = Some(Database::open(cmd[5..].trim())?);
            },
            cmd if cmd.starts_with("rusty-dump") => if let Some(db) = db.as_ref() {
                println!("{:#?}", db);
            }
            cmd => eprintln!("'{}' is not a recognised command", cmd.split_whitespace().next().unwrap()),
        };
        
        Ok(())
    });
}

fn print_errors(mut handler: impl FnMut() -> Result<()>) {
    loop {
        if let Err(e) = handler() {
            eprintln!("{:?}", e);
        }
    }
}

fn prompt(prompt: impl AsRef<str>) -> String {
    let stdin = std::io::stdin();

    eprint!("{}", prompt.as_ref());
    stderr().flush().unwrap();

    let mut buf = String::new();
    if let Ok(len) = stdin.read_line(&mut buf) {
        return buf[..len].to_string();
    };

    String::new()
}
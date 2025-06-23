use fs2::FileExt;
use libdb::error::Result;
use libdb::{Danger, Database};
use std::fs::File;
use std::fs::OpenOptions;
use std::io::stderr;
use std::io::BufRead;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

pub fn main() {
    let mut buf = String::new();

    let mut db = None;

    env_logger::init();

    print_errors(|| {
        match prompt("> ") {
            cmd if cmd.starts_with("open") => {
                let path = PathBuf::from(&cmd[5..].trim());
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(path)?;

                file.lock_exclusive()?;
                if file.metadata()?.size() == 0 {
                    if prompt("Database is empty. Initialise? (y/n) ").trim() == "y" {
                        Database::destructive_reinitialise(&mut file, Danger)?;
                    } else {
                        log::warn!("Database is empty - not opening.");
                        return Ok(());
                    }
                }

                db = Some(DBHandle::new(file)?);
            }
            cmd if cmd.starts_with("rusty-dump") =>
                if let Some(db) = db.as_mut() {
                    println!("{:#?}", db.file_mut().unwrap().db()?);
                },
            cmd if cmd.starts_with("print") => {
                eprintln!("This command is not implemented yet. Use `rusty-dump` instead to view the in-memory state of the database until it's ready");
            }
            cmd if cmd.starts_with("exec") => {
                eprintln!("This command is not implemented yet. Reading and writing to the database is currently being worked on.");
            }
            cmd if cmd.starts_with("exit") => std::process::exit(0),
            cmd => eprintln!("'{}' is not a recognised command", cmd.split_whitespace().next().unwrap()),
        };

        Ok(())
    });
}

#[derive(Debug)]
struct DBHandle {
    backing: Option<File>,
}

impl DBHandle {
    pub fn new(backing: File) -> Result<Self> {
        Ok(Self { backing: Some(backing) })
    }

    fn file_mut(&'_ mut self) -> Option<FileGuard<'_>> {
        let file = self.backing.take()?;
        Some(FileGuard { file: Some(file), handler: self })
    }
}

#[derive(Debug)]
struct FileGuard<'a> {
    file: Option<File>,
    handler: &'a mut DBHandle,
}

impl<'a> Drop for FileGuard<'a> {
    fn drop(&mut self) {
        let file = self.file.take().expect("File was already dropped");
        self.handler.backing = Some(file);
    }
}

impl<'a> FileGuard<'a> {
    pub fn db(&mut self) -> Result<Database<&mut File>> {
        Database::new(self.file.as_mut().expect("File was already dropped"))
    }
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

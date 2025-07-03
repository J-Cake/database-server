use fs2::FileExt;
use libdb::error::Result;
use libdb::{AllocOptions, Danger, Database, FragmentID};
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{stderr, BufRead, BufReader, BufWriter, Read, Seek};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

pub fn main() {
    let mut buf = String::new();

    let mut db = None;

    env_logger::init();

    print_errors(|exit| {
        match prompt("> ") {
            cmd if cmd.starts_with("open-db ") => {
                let path = PathBuf::from(&cmd[8..].trim());
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&path)?;

                file.lock_exclusive()?;
                if file.metadata()?.size() == 0 {
                    if prompt("Database is empty. Initialise? (y/n) ").trim() == "y" {
                        Database::destructive_reinitialise(&mut file, Danger)?;
                    } else {
                        log::warn!("Database is empty - not opening.");
                        return Ok(());
                    }
                }

                let mut handle = DBHandle::new(file)?;

                with_database(&mut handle.file_mut().ok_or(libdb::error::Error::custom("Database not open"))?.db()?, &path);

                db = Some(handle);

            }
            cmd if cmd.starts_with("exit") => {
                drop(db.take());
                *exit = true;
            },
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

impl Drop for DBHandle {
    fn drop(&mut self) {
        if let Some(mut db) = self.file_mut() && let Ok(mut db) = db.db() {
            db.flush()
                .expect("Failed to flush database");
        }

        if let Some(ref mut backing) = self.backing {
            backing.flush()
                .expect("Failed to flush backing buffer");
        }
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

fn print_errors(mut handler: impl FnMut(&mut bool) -> Result<()>) {
    let mut r#break = false;
    while !r#break {
        if let Err(e) = handler(&mut r#break) {
            log::error!("{e:?}");
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

fn with_database(db: &mut Database<&mut File>, path: impl AsRef<std::path::Path>) {
    print_errors(|exit| {
        let cmd = prompt(format!("- ({}) > ", path.as_ref().display()));
        let mut cmd = cmd
            .split_whitespace()
            .peekable();

        match cmd.next() {
            Some("open") => {
                let Some(id) = cmd.next().map(str::parse::<FragmentID>)
                    .transpose()
                    .map_err(libdb::error::Error::from)? else {
                    log::error!("No fragment ID specified");
                    return Ok(());
                };

                let frag = db.open_fragment(id)?;

                with_fragment(frag);
            },
            Some("rusty-dump") => log::debug!("{db:#?}"),
            Some("exit") => *exit = true,
            Some(cmd) => eprintln!("'{cmd}' is not a recognised command"),
            None => ()
        };

        Ok(())
    })
}

fn with_fragment(mut frag: libdb::FragmentHandle<impl Read + Write + Seek>) {
    print_errors(|exit| {
        let cmd = prompt(format!("--- [{}{}] > ", frag.id, 'i'));
        let mut cmd = cmd
            .split_whitespace()
            .peekable();

        match cmd.next() {
            Some("print") => {
                let mut buf = vec![0u8; frag.size().min(1024 * 1024)];
                log::debug!("Reading fragment: {} bytes", frag.size());
                let mut stdout = BufWriter::new(std::io::stdout());

                while let Ok(len) = frag.read(&mut buf) && len > 0 {
                    log::debug!("Read {len} bytes");
                    stdout.write_all(&buf[..len])?;
                }

                stdout.write_all(b"\n")?;

                stdout.flush()?;
            },
            Some("write") => {
                let args = cmd
                    .fold(String::new(), |a, b| a + b + " ")
                    .trim()
                    .to_owned();

                frag.write_all(args.as_bytes())?;
            }
            Some("commit") => {
                frag.flush()?;
                *exit = true;
            },
            Some(cmd) => eprintln!("'{}' is not a recognised command", cmd),
            None => ()
        }

        Ok(())
    })
}
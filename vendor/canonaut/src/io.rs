//! Conversion functions between C FILE* and OutputStream for gradual migration
//!
//! This module provides bridge functions to convert between unsafe C FILE pointers
//! and safe OutputStream enums, allowing for incremental migration of the codebase.

use std::fs::{File, OpenOptions};
use std::io::IsTerminal;
use std::io::Write;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Default)]
pub enum OutputStream {
    #[default]
    StandardOutput,
    StandardError,
    FileOutput(File),
}

impl PartialEq for OutputStream {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (OutputStream::StandardOutput, OutputStream::StandardOutput)
                | (OutputStream::StandardError, OutputStream::StandardError)
                | (OutputStream::FileOutput(_), OutputStream::FileOutput(_))
        )
    }
}

impl OutputStream {
    pub const fn new() -> Self {
        OutputStream::StandardOutput
    }

    pub fn open(path: &str, mode: &str) -> io::Result<Self> {
        let _file = File::open(path)?;
        let file = match mode {
            "a" => OpenOptions::new().append(true).open(path)?,
            "w" => OpenOptions::new().write(true).open(path)?,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid mode")),
        };
        Ok(OutputStream::FileOutput(file))
    }

    pub fn stdout() -> Self {
        OutputStream::StandardOutput
    }

    pub fn stderr() -> Self {
        OutputStream::StandardError
    }

    pub fn create_file(path: &str) -> io::Result<Self> {
        let file = File::create(path)?;
        Ok(OutputStream::FileOutput(file))
    }

    pub fn append_file(path: &str) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(OutputStream::FileOutput(file))
    }

    pub fn is_stdout(&self) -> bool {
        matches!(self, OutputStream::StandardOutput)
    }

    pub fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            OutputStream::StandardOutput => std::io::stdout().write(buffer),
            OutputStream::StandardError => std::io::stderr().write(buffer),
            OutputStream::FileOutput(file) => file.write(buffer),
        }
    }

    pub fn write_str(&mut self, text: &str) -> io::Result<usize> {
        match self {
            OutputStream::StandardOutput => std::io::stdout().write(text.as_bytes()),
            OutputStream::StandardError => std::io::stderr().write(text.as_bytes()),
            OutputStream::FileOutput(file) => file.write(text.as_bytes()),
        }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        match self {
            OutputStream::StandardOutput => std::io::stdout().flush(),
            OutputStream::StandardError => std::io::stderr().flush(),
            OutputStream::FileOutput(file) => file.flush(),
        }
    }

    // pub fn close(&mut self) -> io::Result<()> {
    //     match self {
    //         OutputStream::StandardOutput => Ok(()),
    //         OutputStream::StandardError => Ok(()),
    //         OutputStream::FileOutput(file) => file.close(),
    //     }
    // }
}

pub fn fprintf(stream: &mut OutputStream, args: std::fmt::Arguments) -> io::Result<()> {
    match stream {
        OutputStream::StandardOutput => {
            print!("{}", args);
            Ok(())
        }
        OutputStream::StandardError => {
            eprint!("{}", args);
            Ok(())
        }
        OutputStream::FileOutput(file) => {
            write!(file, "{}", args)
        }
    }
}

pub fn fflush(stream: &mut OutputStream) -> io::Result<()> {
    match stream {
        OutputStream::StandardOutput => io::stdout().flush(),
        OutputStream::StandardError => io::stderr().flush(),
        OutputStream::FileOutput(file) => file.flush(),
    }
}

#[macro_export]
macro_rules! fprintf {
    ($stream:expr, $($arg:tt)*) => {
        $crate::file_conversion::fprintf(&mut $stream, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! fflush {
    ($stream:expr) => {
        $crate::file_conversion::fflush(&mut $stream)
    };
}

/// One input source on the stack — `Stdin` or a `File`, behind one `BufRead` trait object.
pub struct InputSource {
    reader: Box<dyn BufRead>,
    pub is_interactive: bool,
    pub name: String,
    pub line_number: usize, // Useful for error reporting
}

/// The main context that replaces `curfile`, `fileptr[]`, and handling of `ungetc`.
pub struct InputStream {
    stack: Vec<InputSource>,
    pushback_buffer: Vec<u8>, // To emulate ungetc
}

impl Default for InputStream {
    fn default() -> Self {
        Self::new()
    }
}

impl InputStream {
    pub fn new() -> Self {
        let stdin = io::stdin();
        let is_interactive = stdin.is_terminal();

        InputStream {
            stack: vec![InputSource {
                reader: Box::new(BufReader::new(stdin)),
                is_interactive,
                name: "stdin".to_string(),
                line_number: 1,
            }],
            pushback_buffer: Vec::new(),
        }
    }

    /// Replaces C's `getc`.
    /// Returns the character as u32 (to match C's logic of EOF = -1),
    /// or None if the global stack is empty.
    pub fn get_char(&mut self) -> Option<i32> {
        // 1. Check pushback buffer first (LIFO)
        if let Some(character) = self.pushback_buffer.pop() {
            return Some(character as i32);
        }

        // 2. Read from current source
        loop {
            if self.stack.is_empty() {
                return None; // Global EOF
            }

            let last_index = self.stack.len() - 1;
            let source = &mut self.stack[last_index];

            let mut buffer = [0; 1];
            // read() consumes the byte in one call; fill_buf() would need a
            // separate consume() after.
            match source.reader.read(&mut buffer) {
                Ok(0) => {
                    // EOF for this source. Pop it and try the previous one.
                    self.stack.pop();
                    continue;
                }
                Ok(_) => {
                    let character = buffer[0];
                    if character == b'\n' {
                        source.line_number += 1;
                    }
                    return Some(character as i32);
                }
                Err(_) => {
                    // On error, behave like EOF for now
                    self.stack.pop();
                    continue;
                }
            }
        }
    }

    /// Replaces C's `ungetc`.
    pub fn unget_char(&mut self, character: i32) {
        if character != -1 {
            // Ignore EOF markers often passed in C logic
            self.pushback_buffer.push(character as u8);
        }
    }
    /// Returns the current nesting depth of input files.
    /// 0 means we are reading from the initial source (usually stdin).
    /// 1 means we are inside one included file, etc.
    pub fn current_depth(&self) -> usize {
        // The stack always has at least 1 item (stdin), so depth is length - 1
        self.stack.len().saturating_sub(1)
    }

    /// Replaces logic for handling the `<` command.
    pub fn push_file(&mut self, filename: &str) -> io::Result<()> {
        if self.stack.len() >= 10 {
            return Err(io::Error::other("exceeded maximum input nesting"));
        }

        let file = File::open(Path::new(filename))?;

        self.stack.push(InputSource {
            reader: Box::new(BufReader::new(file)),
            is_interactive: false,
            name: filename.to_string(),
            line_number: 1,
        });

        Ok(())
    }

    /// Flushes the rest of the current line from the input.
    ///
    /// If significant characters (non-whitespace/separators) are found before the newline,
    /// it prints a warning message to stderr showing what was skipped.
    pub fn flush_line(&mut self) {
        let mut warning_started = false;

        // Consume characters until EOF.
        while let Some(character) = self.get_char() {

            // Stop at newline
            if character == '\n' as i32 {
                break;
            }

            if warning_started {
                // We have already triggered the warning, just print the skipped char
                eprint!("{}", character as u8 as char);
            } else if character != ' ' as i32
                && character != '\t' as i32
                && character != 12 // Form Feed '\f' or '\u{c}'
                && character != '\r' as i32
                && character != ',' as i32
            {
                // Found first non-whitespace character, start the warning message
                warning_started = true;
                eprint!("input skipped : '{}", character as u8 as char);
            }
        }

        // Close the quote if we printed a warning
        if warning_started {
            eprintln!("'");
        }
    }
    pub fn is_current_interactive(&self) -> bool {
        self.stack.last().map(|source| source.is_interactive).unwrap_or(false)
    }

    pub fn current_filename(&self) -> &str {
        self.stack.last().map(|source| source.name.as_str()).unwrap_or("")
    }
}

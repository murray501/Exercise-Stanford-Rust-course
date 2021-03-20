use crate::debugger_command::DebuggerCommand;
use crate::inferior::{Inferior, Status};
use crate::dwarf_data::{DwarfData, Error as DwarfError};
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    breakpoints: Vec<usize>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        //Initialize the DwarfData
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        debug_data.print();

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            breakpoints: vec![],
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if self.inferior.is_some() {
                        let pid = self.inferior.as_ref().unwrap().pid();
                        if let Ok(_) = self.inferior.take().unwrap().kill() {
                            println!("Killing running inferior (pid {})", pid);
                        } else {
                            println!("Kill (Invalid Input)");
                        }
                    }

                    if let Some(inferior) = Inferior::new(&self.target, &args, &self.breakpoints) {
                        self.inferior = Some(inferior);
                        if let Ok(status) = self.inferior.as_mut().unwrap().continuee() {
                            status.print(&self.debug_data);
                        } else {
                            println!("Error continue");
                        }
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Quit => {
                    if self.inferior.is_some() {
                        let pid = self.inferior.as_ref().unwrap().pid();
                        let res = self.inferior.take().unwrap().kill();
                        if res.is_ok() {
                            println!("Killing running inferior (pid {})", pid);
                        } else {
                            println!("{:?}", res);
                        }
                    }
                    return;
                }
                DebuggerCommand::Continue => {
                    if self.inferior.is_some() {
                        let status = self.inferior.as_mut().unwrap().continuee();
                        if status.is_ok() {
                            status.unwrap().print(&self.debug_data);
                        } else {
                            println!("inferior is not running. {:?}", status);
                        }
                    } else {
                        println!("inferior is not running.");
                    }
                }
                DebuggerCommand::Backtrace => {
                    if self.inferior.is_some() {
                        let res = self.inferior.as_ref().unwrap().print_backtrace(&self.debug_data);
                        if res.is_err() {
                            println!("back trace fail. {:?}", res);
                        }
                    } else {
                        println!("inferior is not running.")
                    }
                }
                DebuggerCommand::Breakpoint(address) => {
                    if let Some(addr) = address {
                        println!("Set breakpoint {} at {}", self.breakpoints.len(), addr);
                        if self.inferior.is_none() {
                            self.breakpoints.push(addr);
                        } else {
                            self.inferior.as_mut().unwrap().set_breakpoint(addr);
                        }
                    }
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().len() == 0 {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}

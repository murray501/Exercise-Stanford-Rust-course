use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::process::{Child, Command};
use std::os::unix::process::CommandExt;
use std::fmt;
use crate::dwarf_data::{DwarfData, Line};

#[derive(Debug)]
pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Status::Stopped(sig1,_), Status::Stopped(sig2,_)) => sig1 == sig2,
            (Status::Exited(code1), Status::Exited(code2)) => code1 == code2,
            (Status::Signaled(sig1), Status::Signaled(sig2)) => sig1 == sig2,
            _ => false,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Stopped(signal, usize) => write!(f, "Child stopped (signal {})", signal.to_string()),
            Status::Exited(code) => write!(f, "Child exited (status = {})", code),
            Status::Signaled(signal) => write!(f, "Child signaled (signal {})", signal.to_string()),
        }
    }
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

pub struct Inferior {
    child: Child,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>) -> Option<Inferior> {
        let child = unsafe { Command::new(target)
            .args(args)
            .pre_exec(child_traceme)
            .spawn()
            .ok()? };

        let me = Inferior { child };
        let status = me.wait(None).ok()?;
        if status == Status::Stopped(signal::Signal::SIGTRAP, 0) {
            return Some(me);
        } else {
            return None;
        }
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => {
                Status::Exited(exit_code)
            },
            WaitStatus::Signaled(_pid, signal, _core_dumped) => {
                Status::Signaled(signal)
            },
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            },
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn continuee(&self) -> Result<Status, nix::Error> {
        ptrace::cont(self.pid(), None)?;
        self.wait(None)
    }

    pub fn kill(&mut self) -> Result<Status, nix::Error> {
        //self.child.kill()?;
        ptrace::kill(self.pid())?;
        self.wait(None)
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error> {
        let regs = ptrace::getregs(self.pid())?;
        let mut rip: usize = regs.rip as usize;
        let mut rbp: usize = regs.rbp as usize;

        loop {
            let line: Line = debug_data.get_line_from_addr(rip).expect("get_line_from_addr fail.");
            let name = debug_data.get_function_from_addr(rip).expect("get_func_from_addr fail.");
            println!("{} ({}:{})", name, line.file, line.number);
            if name == "main" {
                break;
            }
            rip = ptrace::read(self.pid(), (rbp + 8) as ptrace::AddressType)? as usize;
            rbp = ptrace::read(self.pid(), rbp as ptrace::AddressType)? as usize;
        }

        Ok(())
    }
}

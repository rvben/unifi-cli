use std::env;
use std::fmt::Display;
use std::process::{self, Command};

const COMMAND: &str = "unifi";

fn fail(error: impl Display) -> ! {
    eprintln!("unifi-cli: failed to launch {COMMAND}: {error}");
    process::exit(126);
}

fn main() {
    let mut executable = env::current_exe().unwrap_or_else(|error| fail(error));
    executable.set_file_name(format!("{COMMAND}{}", env::consts::EXE_SUFFIX));

    let mut command = Command::new(executable);
    command.args(env::args_os().skip(1));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        fail(command.exec());
    }

    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) => fail(error),
    }
}

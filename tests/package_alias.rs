use std::process::{Command, Output};

fn run(executable: &str, argument: &str) -> Output {
    Command::new(executable)
        .arg(argument)
        .output()
        .expect("CLI should start")
}

#[test]
fn package_named_alias_forwards_arguments_output_and_status() {
    for argument in ["--version", "--__alias-test-invalid"] {
        let primary = run(env!("CARGO_BIN_EXE_unifi"), argument);
        let alias = run(env!("CARGO_BIN_EXE_unifi-cli"), argument);

        assert_eq!(alias.status.code(), primary.status.code());
        assert_eq!(alias.stdout, primary.stdout);
        assert_eq!(alias.stderr, primary.stderr);
    }
}

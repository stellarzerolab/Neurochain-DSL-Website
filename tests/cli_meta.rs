use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn cli_argument_commands_work() {
    #[allow(deprecated)]
    let mut help = Command::cargo_bin("neurochain").expect("bin build");
    help.arg("help")
        .assert()
        .success()
        .stdout(contains("macro from AI:"));

    #[allow(deprecated)]
    let mut version = Command::cargo_bin("neurochain").expect("bin build");
    version
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("NeuroChain version"));

    #[allow(deprecated)]
    let mut about = Command::cargo_bin("neurochain").expect("bin build");
    about
        .arg("--about")
        .assert()
        .success()
        .stdout(contains("NeuroChain CLI"));
}

#[test]
fn cli_interactive_meta_commands_work() {
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain").expect("bin build");

    // Interactive mode reads a "block" until an empty line.
    // So each command here is followed by a blank line.
    cmd.write_stdin("version\n\nabout\n\nhelp\n\nexit\n\n")
        .assert()
        .success()
        .stdout(contains("NeuroChain version"))
        .stdout(contains("NeuroChain CLI"))
        .stdout(contains("macro from AI:"))
        .stdout(contains("Exiting"));
}

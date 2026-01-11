use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn undefined_variable_causes_exit() {
    // Run the binary with an invalid script and ensure it does not crash,
    // and still prints the final "script finished" line (current behavior).
    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain").expect("bin build");

    cmd.args(["examples/error_undefined_variable.nc"])
        .assert()
        .success()
        .stdout(contains("Script finished"));
}

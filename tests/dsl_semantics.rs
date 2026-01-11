use assert_cmd::Command;
use predicates::str::contains;
use tempfile::NamedTempFile;

#[test]
fn dsl_semantics_trim_and_numeric_add() {
    let mut file = NamedTempFile::new().expect("temp file");
    std::io::Write::write_all(
        &mut file,
        br#"
neuro "=== DSL SEMANTICS START ==="

# Equality is case-insensitive and trims whitespace.
set a = "OK"
set b = " ok "
if a == b:
    neuro "PASS trim+case"
else:
    neuro "FAIL trim+case"

# If both sides look numeric, '+' becomes numeric addition even when written as strings.
set sum = "4" + "2"
neuro sum

# Unary minus + decimals
set neg = -2
neuro neg
if -2 < 0:
    neuro "NEG PASS"

set pi = 3.14
neuro pi
set dec = 3.14 + 0.86
neuro dec

neuro "=== DSL SEMANTICS END ==="
"#,
    )
    .expect("write script");

    #[allow(deprecated)]
    let mut cmd = Command::cargo_bin("neurochain").expect("bin build");

    cmd.arg(file.path())
        .assert()
        .success()
        .stdout(contains("neuro: PASS trim+case"))
        .stdout(contains("neuro: 6"))
        .stdout(contains("neuro: -2"))
        .stdout(contains("neuro: NEG PASS"))
        .stdout(contains("neuro: 3.14"))
        .stdout(contains("neuro: 4"));
}

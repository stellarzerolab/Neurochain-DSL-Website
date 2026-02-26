use crate::interpreter::Interpreter;
use crate::lexer::tokenize;
use crate::parser::parse;

/// Lexer → Parser → Interpreter – one block at a time.
pub fn analyze_blocks(input: &str, interpreter: &mut Interpreter) -> Result<(), String> {
    let mut current_block = String::new();

    for line in input.lines() {
        if line.trim().is_empty() {
            if !current_block.trim().is_empty() {
                run_single_block(&current_block, interpreter)?;
                current_block.clear();
            }
            continue;
        }
        current_block.push_str(line);
        current_block.push('\n');
    }

    if !current_block.trim().is_empty() {
        run_single_block(&current_block, interpreter)?;
    }

    Ok(())
}

fn run_single_block(block: &str, interpreter: &mut Interpreter) -> Result<(), String> {
    let tokens = tokenize(block)?; // Lexer already handles debug output.
    let ast = parse(tokens);
    interpreter.run(ast);
    Ok(())
}

/// Runs the entire input as a single block (currently unused).
#[allow(dead_code)]
pub fn analyze(input: &str, interpreter: &mut Interpreter) -> Result<String, String> {
    interpreter.clear_output();
    run_single_block(input, interpreter)?;
    let out = interpreter.take_output();
    if out.trim().is_empty() {
        Ok("Execution succeeded.".into())
    } else {
        Ok(out)
    }
}

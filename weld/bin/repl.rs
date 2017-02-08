extern crate rustyline;
extern crate easy_ll;
extern crate weld;

use rustyline::error::ReadlineError;
use rustyline::Editor;
use std::env;
use std::path::PathBuf;

use weld::*;
use weld::llvm::LlvmGenerator;
use weld::parser::*;
use weld::pretty_print::*;
use weld::type_inference::*;
use weld::sir::ast_to_sir;

// To prevent de-duplication. Really annoying to have these everywhere...how to prevent them from
// being optimized out?
fn runtime_functions() {
    weld_rt_free(0, weld_rt_realloc(0, weld_rt_malloc(0, 16), 32));
    weld_rt_set_errno(-1, WeldRuntimeErrno::Success);
    weld_rt_get_errno(-1);
}

fn main() {
    let home_path = env::home_dir().unwrap_or(PathBuf::new());
    let history_file_path = home_path.join(".weld_history");
    let history_file_path = history_file_path.to_str().unwrap_or(".weld_history");

    runtime_functions();

    let mut rl = Editor::<()>::new();
    if let Err(_) = rl.load_history(&history_file_path) {}

    loop {
        let raw_readline = rl.readline(">> ");
        let readline;
        match raw_readline {
            Ok(raw_readline) => {
                rl.add_history_entry(&raw_readline);
                readline = raw_readline;
            }
            Err(ReadlineError::Interrupted) => {
                println!("Exiting!");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Exiting!");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }

        let trimmed = readline.trim();
        if trimmed == "" {
            continue;
        }

        let program = parse_program(trimmed);
        if let Err(ref e) = program {
            println!("Error during parsing: {:?}", e);
            continue;
        }
        let program = program.unwrap();
        println!("Raw structure:\n{:?}\n", program);

        let expr = macro_processor::process_program(&program);
        if let Err(ref e) = expr {
            println!("Error during macro substitution: {}", e);
            continue;
        }
        let mut expr = expr.unwrap();
        println!("After macro substitution:\n{}\n", print_expr(&expr));

        transforms::inline_apply(&mut expr);
        println!("After inline_apply:\n{}\n", print_expr(&expr));

        transforms::uniquify(&mut expr);
        println!("After uniquify :\n{}\n", print_expr(&expr));

        if let Err(ref e) = infer_types(&mut expr) {
            println!("Error during type inference: {}\n", e);
            println!("Partially inferred types:\n{}\n", print_typed_expr(&expr));
            continue;
        }
        println!("After type inference:\n{}\n", print_typed_expr(&expr));
        println!("Expression type: {}\n", print_type(&expr.ty));

        let mut expr = expr.to_typed().unwrap();

        transforms::inline_zips(&mut expr);
        println!("After inlining zips:\n{}\n", print_typed_expr(&expr));

        transforms::fuse_loops_horizontal(&mut expr);
        println!("After horizontal loop fusion:\n{}\n",
                 print_typed_expr(&expr));

        transforms::fuse_loops_vertical(&mut expr);
        println!("After vertical loop fusion:\n{}\n", print_typed_expr(&expr));

        println!("final program raw: {:?}", expr);

        let sir_result = ast_to_sir(&expr);
        match sir_result {
            Ok(sir) => {
                println!("SIR representation:\n{}\n", &sir);
                let mut llvm_gen = LlvmGenerator::new();
                if let Err(ref e) = llvm_gen.add_function_on_pointers("run", &sir) {
                    println!("Error during LLVM code gen:\n{}\n", e);
                } else {
                    let llvm_code = llvm_gen.result();
                    println!("LLVM code:\n{}\n", llvm_code);

                    if let Err(ref e) = easy_ll::compile_module(&llvm_code) {
                        println!("Error during LLVM compilation:\n{}\n", e);
                    } else {
                        println!("LLVM module compiled successfully\n");
                    }
                }
            }
            Err(ref e) => {
                println!("Error during SIR code gen:\n{}\n", e);
            }
        }
    }
    rl.save_history(&history_file_path).unwrap();
}

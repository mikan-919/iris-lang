mod ast;
mod error;
mod lexer;
mod parser;
mod typecheck;

use miette::{NamedSource, Result};

fn main() -> Result<()> {
    // サンプルコード
    let source = r#"
fn double(n: Int): Int -> n :: * 2

let result = double(21) :: * ""
"#;

    let named_src = NamedSource::new("example.iris", source.to_string());
    let lexer = lexer::Lexer::new(source);

    match parser::ProgramParser::new().parse(lexer) {
        Ok(program) => {
            println!("✅ Parsed {} top-level items", program.len());
            for item in &program {
                println!("  {:?}", item);
            }

            println!("\n--- Running Type Checker ---");
            let mut checker = typecheck::TypeChecker::new();
            let mut env = typecheck::TypeEnv::new();

            if let Err(err) = checker.check_program(&mut env, &program) {
                let diag = error::convert_type_error(named_src, err);
                return Err(miette::Report::new(diag));
            }

            println!("✅ Type checking completed successfully!");
            println!("\nInferred Top-Level Variables/Functions:");
            for item in &program {
                match item {
                    ast::Item::LetStmt(let_stmt) => {
                        if let Some(ty) = env.lookup(&let_stmt.name) {
                            println!("  let {}: {:?}", let_stmt.name, ty);
                        }
                    }
                    ast::Item::FuncDef(func_def) => {
                        if let Some(ty) = env.lookup(&func_def.name) {
                            println!("  fn {}: {:?}", func_def.name, ty);
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        Err(err) => {
            let diag = error::convert_parse_error(named_src, err);
            Err(miette::Report::new(diag))
        }
    }
}

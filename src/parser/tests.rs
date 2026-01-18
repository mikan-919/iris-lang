use crate::lexer::tokenizer::Lexer;
use crate::parser::Parser;

#[test]
fn test_statement_level_for_loop() {
    let code = "for item@list :: item";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    if let Err(e) = &result {
        eprintln!("Parse error: {}", e);
    }
    assert!(result.is_ok(), "Expected Ok, got Err: {:?}", result);
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        crate::ast::Stmt::ForLoopStatement(for_loop) => {
            assert_eq!(for_loop.item, "item");
            assert_eq!(format!("{}", for_loop.collection), "list");
            assert_eq!(format!("{}", for_loop.body), "item");
        }
        _ => panic!("Expected ForLoopStatement"),
    }
}

#[test]
fn test_pipeline_for_loop() {
    let code = "list :: for item@list :: item";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_err());
}

#[test]
fn test_pipeline_for_loop_in_binding() {
    let code = "let result =: list :: for item@list :: item";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        crate::ast::Stmt::Binding { name, expr } => {
            assert_eq!(name, "result");
            match expr {
                crate::ast::Expr::Pipeline { initial, steps } => {
                    assert_eq!(format!("{}", initial), "list");
                    assert_eq!(steps.len(), 1);
                    match &steps[0] {
                        crate::ast::PipelineStep::ForLoop(for_loop) => {
                            assert_eq!(for_loop.item, "item");
                            assert_eq!(format!("{}", for_loop.collection), "list");
                            assert_eq!(format!("{}", for_loop.body), "item");
                        }
                        _ => panic!("Expected ForLoop step"),
                    }
                }
                _ => panic!("Expected Pipeline"),
            }
        }
        _ => panic!("Expected Binding"),
    }
}

#[test]
fn test_for_keyword_tokenization() {
    let mut lexer = Lexer::new("for item@list");
    let tokens = lexer.tokenize();
    assert_eq!(tokens[0].kind, crate::lexer::TokenKind::For);
    assert_eq!(
        tokens[1].kind,
        crate::lexer::TokenKind::Identifier("item".to_string())
    );
    assert_eq!(tokens[2].kind, crate::lexer::TokenKind::At);
    assert_eq!(
        tokens[3].kind,
        crate::lexer::TokenKind::Identifier("list".to_string())
    );
}

#[test]
fn test_match_keyword_tokenization() {
    let mut lexer = Lexer::new("match value");
    let tokens = lexer.tokenize();
    assert_eq!(tokens[0].kind, crate::lexer::TokenKind::Match);
    assert_eq!(
        tokens[1].kind,
        crate::lexer::TokenKind::Identifier("value".to_string())
    );
}

#[test]
fn test_function_definition_expression_style() {
    let code = "fn add(a: Int, b: Int) -> Int =: a + b";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        crate::ast::Stmt::FunctionDefinition {
            name,
            is_exported,
            params,
            return_type,
            body: _,
        } => {
            assert_eq!(name, "add");
            assert!(!*is_exported);
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].0, "a");
            assert_eq!(params[1].0, "b");
            assert!(matches!(return_type.normalize(), crate::ast::Type::Int));
        }
        _ => panic!("Expected FunctionDefinition"),
    }
}

#[test]
fn test_function_definition_with_pipeline() {
    let code = r#"fn greet(name: String) -> String =: "Hello, " :: concat(name)"#;
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        crate::ast::Stmt::FunctionDefinition {
            name,
            is_exported,
            params,
            return_type,
            body: _,
        } => {
            assert_eq!(name, "greet");
            assert!(!*is_exported);
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "name");
            assert!(matches!(return_type.normalize(), crate::ast::Type::String));
        }
        _ => panic!("Expected FunctionDefinition"),
    }
}

#[test]
fn test_function_definition_literal() {
    let code = "fn get_constant() -> Int =: 42";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok());
    let stmts = result.unwrap();
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        crate::ast::Stmt::FunctionDefinition {
            name,
            is_exported,
            params,
            return_type,
            body: _,
        } => {
            assert_eq!(name, "get_constant");
            assert!(!*is_exported);
            assert_eq!(params.len(), 0);
            assert!(matches!(return_type.normalize(), crate::ast::Type::Int));
        }
        _ => panic!("Expected FunctionDefinition"),
    }
}

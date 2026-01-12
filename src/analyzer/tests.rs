use crate::analyzer::TypeChecker;
use crate::lexer::tokenizer::Lexer;
use crate::parser::Parser;

#[test]
fn test_simple_type_inference() {
    let code = "let x =: 10";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let stmts = result.unwrap();

    let mut type_checker = TypeChecker::new();
    let check_result = type_checker.check(&stmts);
    assert!(
        check_result.is_ok(),
        "Type check error: {:?}",
        check_result.err()
    );
}

#[test]
fn test_pipeline_type_inference() {
    let code = "
        fn takes_any(x: Any) -> Any =: x

        let result =: 10 :: takes_any()
    ";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let stmts = result.unwrap();

    let mut type_checker = TypeChecker::new();
    let check_result = type_checker.check(&stmts);

    assert!(
        check_result.is_ok(),
        "Type check error: {:?}",
        check_result.err()
    );
}

#[test]
fn test_type_mismatch_error() {
    let code = "
        fn takes_string(s: String) -> Int =: 0

        let x =: 123
        let y =: x :: takes_string()
    ";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let stmts = result.unwrap();

    let mut type_checker = TypeChecker::new();
    let check_result = type_checker.check(&stmts);
    assert!(check_result.is_err(), "Expected type mismatch error");
    let error_msg = check_result.unwrap_err();
    assert!(
        error_msg.contains("type mismatch") || error_msg.contains("mismatch"),
        "Expected type mismatch error, got: {}",
        error_msg
    );
}

#[test]
fn test_undefined_function_error() {
    let code = "let x =: 10 :: unknown_function()";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let stmts = result.unwrap();

    let mut type_checker = TypeChecker::new();
    let check_result = type_checker.check(&stmts);
    assert!(check_result.is_err(), "Expected undefined function error");
    let error_msg = check_result.unwrap_err();
    assert!(
        error_msg.contains("Undefined function"),
        "Expected undefined function error, got: {}",
        error_msg
    );
}

#[test]
fn test_arity_mismatch_error() {
    let code = "
        fn takes_two(a: Int, b: Int) -> Int =: a

        let x =: 10 :: takes_two()
    ";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let stmts = result.unwrap();

    let mut type_checker = TypeChecker::new();
    let check_result = type_checker.check(&stmts);
    assert!(check_result.is_err(), "Expected arity mismatch error");
    let error_msg = check_result.unwrap_err();
    assert!(
        error_msg.contains("1 argument"),
        "Expected arity error, got: {}",
        error_msg
    );
}

#[test]
fn test_tuple_merge() {
    let code = "
        let x =: 1
        let y =: \"hello\"
        let z =: x :& y
    ";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let result = parser.parse();
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let stmts = result.unwrap();

    let mut type_checker = TypeChecker::new();
    let check_result = type_checker.check(&stmts);
    assert!(
        check_result.is_ok(),
        "Type check error: {:?}",
        check_result.err()
    );
}

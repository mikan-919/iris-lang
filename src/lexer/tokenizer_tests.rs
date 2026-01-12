use crate::lexer::TokenKind;
use crate::lexer::tokenizer::Lexer;

#[test]
fn test_bind_operator() {
    let mut lexer = Lexer::new("=: ::");
    let tokens = lexer.tokenize();
    assert_eq!(tokens[0].kind, TokenKind::Bind);
    assert_eq!(tokens[1].kind, TokenKind::Next);
}

#[test]
fn test_simple_pipeline() {
    let mut lexer = Lexer::new("let result =: 10 :: double()");
    let tokens = lexer.tokenize();
    assert_eq!(tokens[0].kind, TokenKind::Let);
    assert_eq!(tokens[1].kind, TokenKind::Identifier("result".to_string()));
    assert_eq!(tokens[2].kind, TokenKind::Bind);
    assert_eq!(tokens[3].kind, TokenKind::Integer(10));
    assert_eq!(tokens[4].kind, TokenKind::Next);
}

#[test]
fn test_all_backbone_operators() {
    let code = "=: :: :~ :^ :! :? :| :> :&";
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize();
    let kinds: Vec<_> = tokens
        .iter()
        .map(|t| &t.kind)
        .filter(|k| !matches!(k, TokenKind::EOF))
        .collect();
    assert_eq!(
        kinds,
        &[
            &TokenKind::Bind,
            &TokenKind::Next,
            &TokenKind::Await,
            &TokenKind::Try,
            &TokenKind::Force,
            &TokenKind::Catch,
            &TokenKind::Or,
            &TokenKind::Tag,
            &TokenKind::Join,
        ]
    );
}

#[test]
fn test_export_keyword() {
    let mut lexer = Lexer::new("export fn add");
    let tokens = lexer.tokenize();
    assert_eq!(tokens[0].kind, TokenKind::Export);
    assert_eq!(tokens[1].kind, TokenKind::Fn);
    assert_eq!(tokens[2].kind, TokenKind::Identifier("add".to_string()));
}

#[test]
fn test_pipe_and_at() {
    let mut lexer = Lexer::new("item@list | pattern");
    let tokens = lexer.tokenize();
    assert_eq!(tokens[0].kind, TokenKind::Identifier("item".to_string()));
    assert_eq!(tokens[1].kind, TokenKind::At);
    assert_eq!(tokens[2].kind, TokenKind::Identifier("list".to_string()));
    assert_eq!(tokens[3].kind, TokenKind::Pipe);
    assert_eq!(tokens[4].kind, TokenKind::Identifier("pattern".to_string()));
}

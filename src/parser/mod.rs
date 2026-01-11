use crate::ast::{Expr, Literal, MatchArm, PipelineStep, Stmt, Type};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    pub fn parse(&mut self) -> Result<Vec<Stmt>, String> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            if let TokenKind::EOF = self.peek().kind {
                break;
            }
            if let TokenKind::Whitespace | TokenKind::Newline = self.peek().kind {
                self.advance();
                continue;
            }

            let stmt = self.parse_statement()?;
            statements.push(stmt);
        }

        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Stmt, String> {
        match self.peek().kind {
            TokenKind::Let => self.parse_binding(),
            TokenKind::Export => {
                self.advance();
                if self.check(TokenKind::Fn) {
                    self.parse_function_definition()
                } else {
                    Err(format!(
                        "expected 'fn' after 'export', found {}",
                        self.peek()
                    ))
                }
            }
            TokenKind::Fn => self.parse_function_definition(),
            TokenKind::For => {
                self.advance();
                let for_loop = self.parse_for_loop()?;
                Ok(Stmt::ForLoopStatement(Box::new(for_loop)))
            }
            _ => Err(format!("expected statement, found {}", self.peek())),
        }
    }

    fn parse_binding(&mut self) -> Result<Stmt, String> {
        self.consume(TokenKind::Let, "expected 'let'")?;

        let name = self.consume_identifier()?;

        self.consume(TokenKind::Bind, "expected '=:'")?;

        let expr = self.parse_expression()?;

        Ok(Stmt::Binding { name, expr })
    }

    fn parse_function_definition(&mut self) -> Result<Stmt, String> {
        self.consume(TokenKind::Fn, "expected 'fn'")?;

        let name = self.consume_identifier()?;

        self.consume(TokenKind::LParen, "expected '('")?;

        let mut params = Vec::new();
        if self.peek().kind != TokenKind::RParen {
            loop {
                let param_name = self.consume_identifier()?;
                let param_type = if self.match_token(TokenKind::Colon) {
                    let type_name = self.consume_identifier()?;
                    Type::Simple(type_name)
                } else {
                    Type::Simple("Any".to_string())
                };
                params.push((param_name, param_type));
                if !self.match_token(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.consume(TokenKind::RParen, "expected ')'")?;

        self.consume(TokenKind::Arrow, "expected '->'")?;

        let return_type_name = self.consume_identifier()?;
        let return_type = Type::Simple(return_type_name);

        self.consume(TokenKind::Bind, "expected '=:'")?;

        let expr = self.parse_expression()?;

        Ok(Stmt::FunctionDefinition {
            name,
            params,
            return_type,
            body: Box::new(expr),
        })
    }

    fn parse_expression(&mut self) -> Result<Expr, String> {
        if self.check(TokenKind::Match) {
            return self.parse_match();
        }

        let initial = self.parse_term()?;

        let mut steps = Vec::new();

        loop {
            match self.peek().kind {
                TokenKind::Next => {
                    self.advance();
                    if self.check(TokenKind::For) {
                        self.advance();
                        let for_loop = self.parse_for_loop()?;
                        steps.push(PipelineStep::ForLoop(Box::new(for_loop)));
                    } else if let TokenKind::Identifier(name) = self.peek().kind.clone() {
                        self.advance();
                        self.consume(TokenKind::LParen, "expected '('")?;
                        self.consume(TokenKind::RParen, "expected ')'")?;
                        steps.push(PipelineStep::FunctionCall(name));
                    } else {
                        return Err(format!(
                            "expected function name or 'for' after ::, found {}",
                            self.peek()
                        ));
                    }
                }
                TokenKind::Await => {
                    self.advance();
                    let func_name = self.consume_identifier()?;
                    self.consume(TokenKind::LParen, "expected '(' after :~")?;
                    self.consume(TokenKind::RParen, "expected ')' after function name")?;
                    steps.push(PipelineStep::AsyncCall(func_name));
                }
                TokenKind::Try => {
                    self.advance();
                    steps.push(PipelineStep::ErrorPropagate);
                }
                TokenKind::Force => {
                    self.advance();
                    steps.push(PipelineStep::Force);
                }
                TokenKind::Catch => {
                    self.advance();
                    let expr = self.parse_term()?;
                    steps.push(PipelineStep::ErrorRescue(Box::new(expr)));
                }
                TokenKind::Or => {
                    self.advance();
                    let expr = self.parse_term()?;
                    steps.push(PipelineStep::Fallback(Box::new(expr)));
                }
                TokenKind::Tag => {
                    self.advance();
                    let name = self.consume_identifier()?;
                    steps.push(PipelineStep::BorrowReference(name));
                }
                TokenKind::Join => {
                    self.advance();
                    let expr = self.parse_term()?;
                    steps.push(PipelineStep::TupleMerge(Box::new(expr)));
                }
                TokenKind::Pipe => {
                    let arm = self.parse_match_arm()?;
                    steps.push(PipelineStep::MatchArm(Box::new(arm)));
                }
                _ => break,
            }
        }

        if steps.is_empty() {
            Ok(initial)
        } else {
            Ok(Expr::Pipeline {
                initial: Box::new(initial),
                steps,
            })
        }
    }

    fn parse_match(&mut self) -> Result<Expr, String> {
        self.consume(TokenKind::Match, "expected 'match'")?;

        let subject = self.parse_term()?;

        let mut arms = Vec::new();

        while self.check(TokenKind::Pipe) {
            self.advance();
            let arm = self.parse_match_arm()?;
            arms.push(arm);
        }

        Ok(Expr::Match {
            subject: Box::new(subject),
            arms,
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(_) => Ok(MatchArm::Pattern {
                name: self.consume_identifier()?,
                args: if self.check(TokenKind::LParen) {
                    self.consume(TokenKind::LParen, "expected '('")?;
                    let mut args = Vec::new();
                    if self.peek().kind != TokenKind::RParen {
                        loop {
                            let arg = self.parse_term()?;
                            args.push(arg);
                            if !self.match_token(TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    self.consume(TokenKind::RParen, "expected ')'")?;
                    args
                } else {
                    Vec::new()
                },
            }),
            _ => {
                let expr = self.parse_term()?;
                Ok(MatchArm::Expression(Box::new(expr)))
            }
        }
    }

    fn parse_for_loop(&mut self) -> Result<crate::ast::ForLoop, String> {
        let item = self.consume_identifier()?;
        self.consume(TokenKind::At, "expected '@' after item name")?;
        let collection = self.parse_term()?;
        self.consume(TokenKind::Next, "expected '::' after collection")?;
        let body = self.parse_term()?;

        Ok(crate::ast::ForLoop {
            subject: Box::new(Expr::Literal(Literal::Integer(0))),
            item,
            collection: Box::new(collection),
            body: Box::new(body),
            is_threading: false,
        })
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        match self.peek().kind.clone() {
            TokenKind::Integer(n) => {
                self.advance();
                Ok(Expr::Literal(Literal::Integer(n)))
            }
            TokenKind::String(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::String(s)))
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Expr::Identifier(name))
            }
            _ => Err(format!("expected term, found {}", self.peek())),
        }
    }

    fn consume_identifier(&mut self) -> Result<String, String> {
        if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();
            Ok(name)
        } else {
            Err(format!("expected identifier, found {}", self.peek()))
        }
    }

    fn consume(&mut self, kind: TokenKind, message: &str) -> Result<(), String> {
        if self.check(kind) {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "{} at {}, found {}",
                message,
                self.peek().line,
                self.peek()
            ))
        }
    }

    fn match_token(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        if self.is_at_end() {
            return false;
        }
        self.peek().kind == kind
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.current += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::EOF
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenizer::Lexer;

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
            Stmt::ForLoopStatement(for_loop) => {
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
        // This should fail because "list :: for item@list :: item" is not a complete statement
        // It needs to be part of a binding
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
            Stmt::Binding { name, expr } => {
                assert_eq!(name, "result");
                match expr {
                    Expr::Pipeline { initial, steps } => {
                        assert_eq!(format!("{}", initial), "list");
                        assert_eq!(steps.len(), 1);
                        match &steps[0] {
                            PipelineStep::ForLoop(for_loop) => {
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
        assert_eq!(tokens[0].kind, TokenKind::For);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("item".to_string()));
        assert_eq!(tokens[2].kind, TokenKind::At);
        assert_eq!(tokens[3].kind, TokenKind::Identifier("list".to_string()));
    }

    #[test]
    fn test_match_keyword_tokenization() {
        let mut lexer = Lexer::new("match value");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0].kind, TokenKind::Match);
        assert_eq!(tokens[1].kind, TokenKind::Identifier("value".to_string()));
    }
}

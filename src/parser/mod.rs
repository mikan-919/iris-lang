use crate::ast::{Expr, Literal, PipelineStep, Stmt, Type};
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
        let initial = self.parse_term()?;
        let mut steps = Vec::new();

        loop {
            match self.peek().kind {
                TokenKind::Next => {
                    self.advance();
                    let step = self.parse_pipeline_step()?;
                    steps.push(step);
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

    fn parse_pipeline_step(&mut self) -> Result<PipelineStep, String> {
        if let TokenKind::Identifier(name) = self.peek().kind.clone() {
            self.advance();

            if self.match_token(TokenKind::LParen) {
                self.consume(TokenKind::RParen, "expected ')'")?;
                Ok(PipelineStep::FunctionCall(name))
            } else {
                Ok(PipelineStep::FunctionCall(name))
            }
        } else {
            Err(format!(
                "expected function name in pipeline, found {}",
                self.peek()
            ))
        }
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
                if self.match_token(TokenKind::LParen) {
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
                    Ok(Expr::FunctionCall { name, args })
                } else {
                    Ok(Expr::Identifier(name))
                }
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

    #[test]
    fn test_parse_simple_pipeline() {
        let code = "let result =: 10 :: double()";
        let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        assert_eq!(statements.len(), 1);

        if let Stmt::Binding { name, expr } = &statements[0] {
            assert_eq!(name, "result");
            if let Expr::Pipeline { initial, steps } = expr {
                assert_eq!(initial.as_ref(), &Expr::Literal(Literal::Integer(10)));
                assert_eq!(steps.len(), 1);
                if let PipelineStep::FunctionCall(func_name) = &steps[0] {
                    assert_eq!(func_name, "double");
                } else {
                    panic!("Expected FunctionDefinition");
                }
            }

            #[test]
            fn test_parse_function_with_type_annotations() {
                let code = "fn add(a: Int, b: Int) -> Int =: a";
                let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
                let tokens = lexer.tokenize();
                let mut parser = Parser::new(tokens);
                let statements = parser.parse().unwrap();

                if let Stmt::FunctionDefinition {
                    params,
                    return_type,
                    ..
                } = &statements[0]
                {
                    assert_eq!(params.len(), 2);
                    assert_eq!(params[0].0, "a");
                    assert_eq!(params[0].1, Type::Simple("Int".to_string()));
                    assert_eq!(params[1].0, "b");
                    assert_eq!(params[1].1, Type::Simple("Int".to_string()));
                    assert_eq!(return_type, &Type::Simple("Int".to_string()));
                } else {
                    panic!("Expected FunctionDefinition");
                }
            }
        } else {
            panic!("Expected Binding");
        }
    }

    #[test]
    fn test_parse_await_operator() {
        let code = "let result =: value :~ fetch()";
        let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        if let Stmt::Binding { expr, .. } = &statements[0] {
            if let Expr::Pipeline { steps, .. } = expr {
                assert!(matches!(&steps[0], PipelineStep::AsyncCall(name) if name == "fetch"));
            } else {
                panic!("Expected Pipeline");
            }
        } else {
            panic!("Expected Binding");
        }
    }

    #[test]
    fn test_parse_try_operator() {
        let code = "let result =: risky :: parse() :^";
        let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        if let Stmt::Binding { expr, .. } = &statements[0] {
            if let Expr::Pipeline { steps, .. } = expr {
                assert!(matches!(steps[1], PipelineStep::ErrorPropagate));
            } else {
                panic!("Expected Pipeline");
            }
        } else {
            panic!("Expected Binding");
        }
    }

    #[test]
    fn test_parse_tag_operator() {
        let code = "let result =: 10 :: double() :> snapshot";
        let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        if let Stmt::Binding { expr, .. } = &statements[0] {
            if let Expr::Pipeline { steps, .. } = expr {
                if let PipelineStep::BorrowReference(name) = &steps[1] {
                    assert_eq!(name, "snapshot");
                } else {
                    panic!("Expected BorrowReference");
                }
            } else {
                panic!("Expected Pipeline");
            }
        } else {
            panic!("Expected Binding");
        }
    }

    #[test]
    fn test_parse_or_operator() {
        let code = "let result =: maybe :: getValue() :| 0";
        let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        if let Stmt::Binding { expr, .. } = &statements[0] {
            if let Expr::Pipeline { steps, .. } = expr {
                if let PipelineStep::Fallback(expr) = &steps[1] {
                    assert_eq!(expr.as_ref(), &Expr::Literal(Literal::Integer(0)));
                } else {
                    panic!("Expected Fallback");
                }
            } else {
                panic!("Expected Pipeline");
            }
        } else {
            panic!("Expected Binding");
        }
    }

    #[test]
    fn test_parse_export_function() {
        let code = "export fn double(n) -> Int =: n";
        let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        assert_eq!(statements.len(), 1);
        if let Stmt::FunctionDefinition {
            name,
            params,
            return_type,
            ..
        } = &statements[0]
        {
            assert_eq!(name, "double");
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].0, "n");
            assert_eq!(params[0].1, Type::Simple("Any".to_string()));
            assert_eq!(return_type, &Type::Simple("Int".to_string()));
        } else {
            panic!("Expected FunctionDefinition");
        }
    }

    #[test]
    fn test_parse_function_with_type_annotations() {
        let code = "fn add(a: Int, b: Int) -> Int =: a :: + b";
        let mut lexer = crate::lexer::tokenizer::Lexer::new(code);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let statements = parser.parse().unwrap();

        if let Stmt::FunctionDefinition {
            params,
            return_type,
            ..
        } = &statements[0]
        {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].0, "a");
            assert_eq!(params[0].1, Type::Simple("Int".to_string()));
            assert_eq!(params[1].0, "b");
            assert_eq!(params[1].1, Type::Simple("Int".to_string()));
            assert_eq!(return_type, &Type::Simple("Int".to_string()));
        } else {
            panic!("Expected FunctionDefinition");
        }
    }
}

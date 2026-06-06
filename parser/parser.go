// Package parser turns a stream of tokens into an Iris AST.
//
// Grammar summary (informal):
//
//	program     = stmt*
//	stmt        = comptime_directive
//	            | use_stmt
//	            | export fn_decl
//	            | fn_decl
//	            | trait_decl
//	            | impl_decl
//	            | let_stmt
//	            | expr_stmt
//
//	pipeline    = expr pipeline_step*          (subject-first form)
//	            | headless_pipeline            (:: steps inside parens)
//	pipeline_step = backbone_op step_expr
//	step_expr   = call_expr | match_expr | op_application | ident
//
// Errors are accumulated in Program.Errors; parsing continues after each error.
package parser

import (
	"fmt"
	"iris-lang/ast"
	"iris-lang/lexer"
	"iris-lang/token"
	"strconv"
	"strings"
)

// Parser holds the parsing state.
type Parser struct {
	l      *lexer.Lexer
	cur    token.Token // current token
	peek   token.Token // one token look-ahead
	errors []ast.ParseError
}

// New creates a Parser from a Lexer, priming the two-token look-ahead.
func New(l *lexer.Lexer) *Parser {
	p := &Parser{l: l}
	p.advance() // load peek
	p.advance() // load cur (peek shifts to peek)
	return p
}

// ParseProgram parses the entire source and returns the root Program node.
func (p *Parser) ParseProgram() *ast.Program {
	prog := &ast.Program{}
	for p.cur.Kind != token.EOF {
		if stmt := p.parseStatement(); stmt != nil {
			prog.Stmts = append(prog.Stmts, stmt)
		}
	}
	prog.Errors = p.errors
	return prog
}

// ── Statement dispatch ─────────────────────────────────────────────────────────

func (p *Parser) parseStatement() ast.Statement {
	switch p.cur.Kind {
	case token.BANG:
		return p.parseComptimeDirective()
	case token.KW_USE:
		return p.parseUseStatement()
	case token.KW_EXPORT:
		return p.parseExportDecl()
	case token.KW_FN:
		return p.parseFnDeclaration(false)
	case token.KW_TRAIT:
		return p.parseTraitDeclaration()
	case token.KW_IMPL:
		return p.parseImplDeclaration()
	case token.KW_LET:
		return p.parseLetStatement()
	default:
		return p.parseExpressionStatement()
	}
}

// ── Comptime directive: !name(args) ───────────────────────────────────────────

func (p *Parser) parseComptimeDirective() *ast.ComptimeDirective {
	dir := &ast.ComptimeDirective{Tok: p.cur}
	p.advance() // consume '!'

	if p.cur.Kind != token.IDENT {
		p.errorf(p.cur, "expected directive name after '!'")
		return dir
	}
	dir.Name = p.cur.Lexeme
	p.advance()

	if p.cur.Kind == token.LPAREN {
		p.advance() // consume '('
		dir.Args = p.parseArgExprs(token.RPAREN)
		p.expect(token.RPAREN)
	}
	return dir
}

// ── Use statement: use path ────────────────────────────────────────────────────

func (p *Parser) parseUseStatement() *ast.UseStatement {
	stmt := &ast.UseStatement{Tok: p.cur}
	p.advance() // consume 'use'

	var parts []string
	for p.cur.Kind == token.IDENT {
		parts = append(parts, p.cur.Lexeme)
		p.advance()
		if p.cur.Kind == token.OP_NEXT { // ::
			p.advance()
			if p.cur.Kind == token.STAR {
				parts = append(parts, "*")
				p.advance()
				break
			}
		} else {
			break
		}
	}
	stmt.Path = strings.Join(parts, "::")
	return stmt
}

// ── Export declaration: export fn ... ─────────────────────────────────────────

func (p *Parser) parseExportDecl() ast.Statement {
	p.advance() // consume 'export'
	if p.cur.Kind != token.KW_FN {
		p.errorf(p.cur, "expected 'fn' after 'export'")
		return nil
	}
	fn := p.parseFnDeclaration(true)
	return fn
}

// ── Function declaration ───────────────────────────────────────────────────────

func (p *Parser) parseFnDeclaration(exported bool) *ast.FnDeclaration {
	fn := &ast.FnDeclaration{Tok: p.cur, Exported: exported}
	p.advance() // consume 'fn'

	if p.cur.Kind != token.IDENT {
		p.errorf(p.cur, "expected function name")
		return fn
	}
	fn.Name = p.cur.Lexeme
	p.advance()

	if p.cur.Kind == token.LT {
		fn.TypeParams = p.parseTypeParams()
	}

	p.expect(token.LPAREN)
	fn.Params = p.parseFnParams()
	p.expect(token.RPAREN)

	if p.cur.Kind == token.ARROW {
		p.advance()
		fn.ReturnType = p.parseTypeExpr()
	}

	fn.Body = p.parseFnBody()
	return fn
}

// parseFnParams parses a comma-separated list of 'name: Type' pairs.
func (p *Parser) parseFnParams() []ast.FnParam {
	var params []ast.FnParam
	for p.cur.Kind != token.RPAREN && p.cur.Kind != token.EOF {
		if p.cur.Kind != token.IDENT {
			p.errorf(p.cur, "expected parameter name")
			p.advance()
			continue
		}
		param := ast.FnParam{Name: p.cur.Lexeme}
		p.advance()
		if p.cur.Kind == token.COLON {
			p.advance()
			param.Type = p.parseTypeExpr()
		}
		params = append(params, param)
		if p.cur.Kind == token.COMMA {
			p.advance()
		}
	}
	return params
}

// parseTypeParams parses <T: #Trait, U: #A && #B>
func (p *Parser) parseTypeParams() []ast.TypeParam {
	p.advance() // consume '<'
	var params []ast.TypeParam
	for p.cur.Kind != token.GT && p.cur.Kind != token.EOF {
		if p.cur.Kind != token.IDENT {
			p.errorf(p.cur, "expected type parameter name")
			p.advance()
			continue
		}
		tp := ast.TypeParam{Name: p.cur.Lexeme}
		p.advance()
		if p.cur.Kind == token.COLON {
			p.advance()
			tp.Bounds = p.parseTraitBounds()
		}
		params = append(params, tp)
		if p.cur.Kind == token.COMMA {
			p.advance()
		}
	}
	p.expect(token.GT)
	return params
}

// parseTraitBounds parses #Trait (&&  #Trait)*
func (p *Parser) parseTraitBounds() []ast.TraitRef {
	var bounds []ast.TraitRef
	for p.cur.Kind == token.HASH {
		bounds = append(bounds, p.parseTraitRef())
		if p.cur.Kind == token.ANDAND {
			p.advance()
		} else {
			break
		}
	}
	return bounds
}

// parseFnBody parses either a block body { stmts } or an expression body = pipeline.
func (p *Parser) parseFnBody() ast.FnBody {
	switch p.cur.Kind {
	case token.LBRACE:
		return p.parseBlockBody()
	case token.EQUAL:
		return p.parseExprBody()
	default:
		// Declaration-only (e.g. trait method with no body)
		return nil
	}
}

func (p *Parser) parseBlockBody() *ast.BlockBody {
	body := &ast.BlockBody{Tok: p.cur}
	p.advance() // consume '{'
	for p.cur.Kind != token.RBRACE && p.cur.Kind != token.EOF {
		if stmt := p.parseStatement(); stmt != nil {
			body.Stmts = append(body.Stmts, stmt)
		}
	}
	p.expect(token.RBRACE)
	return body
}

func (p *Parser) parseExprBody() *ast.ExprBody {
	pipeline := p.parsePipelineFromEqual()
	return &ast.ExprBody{Pipeline: pipeline}
}

// ── Trait declaration ──────────────────────────────────────────────────────────

func (p *Parser) parseTraitDeclaration() *ast.TraitDeclaration {
	decl := &ast.TraitDeclaration{Tok: p.cur}
	p.advance() // consume 'trait'

	ref := p.parseTraitRef()
	decl.Name = ref.Name

	if p.cur.Kind == token.COLON {
		p.advance()
		parent := p.parseTraitRef()
		decl.Parent = &parent
	}

	p.expect(token.LBRACE)
	for p.cur.Kind != token.RBRACE && p.cur.Kind != token.EOF {
		if p.cur.Kind == token.KW_FN {
			decl.Methods = append(decl.Methods, p.parseFnDeclaration(false))
		} else {
			p.errorf(p.cur, "expected 'fn' in trait body")
			p.advance()
		}
	}
	p.expect(token.RBRACE)
	return decl
}

// ── Impl declaration ───────────────────────────────────────────────────────────

func (p *Parser) parseImplDeclaration() *ast.ImplDeclaration {
	decl := &ast.ImplDeclaration{Tok: p.cur}
	p.advance() // consume 'impl'

	decl.Trait = p.parseTraitRef()

	if p.cur.Kind != token.KW_FOR {
		p.errorf(p.cur, "expected 'for' in impl")
	} else {
		p.advance()
	}

	decl.ForType = p.parseTypeExpr()

	p.expect(token.LBRACE)
	for p.cur.Kind != token.RBRACE && p.cur.Kind != token.EOF {
		if p.cur.Kind == token.KW_FN {
			decl.Methods = append(decl.Methods, p.parseFnDeclaration(false))
		} else {
			p.errorf(p.cur, "expected 'fn' in impl body")
			p.advance()
		}
	}
	p.expect(token.RBRACE)
	return decl
}

// ── Let statement: let name = pipeline ────────────────────────────────────────

func (p *Parser) parseLetStatement() *ast.LetStatement {
	stmt := &ast.LetStatement{Tok: p.cur}
	p.advance() // consume 'let'

	if p.cur.Kind != token.IDENT {
		p.errorf(p.cur, "expected name after 'let'")
		return stmt
	}
	stmt.Name = p.cur.Lexeme
	p.advance()

	stmt.Value = p.parsePipelineFromEqual()
	return stmt
}

// ── Expression statement ───────────────────────────────────────────────────────

func (p *Parser) parseExpressionStatement() *ast.ExpressionStatement {
	expr := p.parsePipeline() // returns ast.Expression
	return &ast.ExpressionStatement{Expr: expr}
}

// ── Pipeline parsing ───────────────────────────────────────────────────────────

// parsePipelineFromEqual expects = and then parses the rest as a pipeline.
// Used for let statements and expression function bodies.
func (p *Parser) parsePipelineFromEqual() *ast.Pipeline {
	if p.cur.Kind != token.EQUAL {
		p.errorf(p.cur, "expected '='")
		return &ast.Pipeline{Tok: p.cur}
	}
	op := p.cur
	p.advance() // consume '='

	subject := p.parseExpr()

	pipe := &ast.Pipeline{Tok: op, Subject: subject}
	pipe.Steps = p.parsePipelineSteps()
	return pipe
}

// parsePipeline parses an expression optionally followed by pipeline steps.
// Returns the subject expression directly when there are no steps, or a
// *Pipeline when at least one backbone operator follows.
func (p *Parser) parsePipeline() ast.Expression {
	subject := p.parseExpr()
	steps := p.parsePipelineSteps()
	if len(steps) == 0 {
		return subject
	}
	return &ast.Pipeline{Tok: subject.Pos(), Subject: subject, Steps: steps}
}

// parsePipelineSteps parses zero or more pipeline steps following a subject.
func (p *Parser) parsePipelineSteps() []ast.PipelineStep {
	var steps []ast.PipelineStep
	for p.cur.Kind.IsBackboneOp() {
		steps = append(steps, p.parsePipelineStep())
	}
	return steps
}

// parsePipelineStep parses one backbone-op + optional expression.
func (p *Parser) parsePipelineStep() ast.PipelineStep {
	op := p.cur
	p.advance() // consume backbone op

	// Keyword-suffix ops group multiple ops into a single composite expression.
	switch op.Kind {
	case token.OP_IF:
		return ast.PipelineStep{Op: op, Expr: p.parseIfExpression(op)}
	case token.OP_WHILE:
		return ast.PipelineStep{Op: op, Expr: p.parseWhileExpression(op)}
	case token.OP_CATCH_NAMED:
		return ast.PipelineStep{Op: op, Expr: p.parseCatchNamedExpression(op)}
	case token.OP_AWAIT:
		// :await stands alone; subject is the promise being awaited.
		return ast.PipelineStep{Op: op, Expr: &ast.Identifier{Tok: op, Name: ""}}
	}

	// :try stands alone with no following expression.
	if op.Kind == token.OP_TRY && p.cur.Kind.IsBackboneOp() {
		return ast.PipelineStep{Op: op, Expr: &ast.Identifier{Tok: op, Name: ""}}
	}

	var expr ast.Expression
	switch {
	case p.cur.Kind == token.KW_MATCH:
		expr = p.parseMatchExpression()
	case isArithOp(p.cur.Kind):
		// Operator application: :: * 2  means  subject * 2
		expr = p.parseOperatorApplication()
	default:
		expr = p.parseExpr()
	}
	return ast.PipelineStep{Op: op, Expr: expr}
}

// parseIfExpression parses the :if cond :then body [:else fallback] group.
// The :if token has already been consumed.
func (p *Parser) parseIfExpression(opTok token.Token) *ast.IfExpression {
	e := &ast.IfExpression{Tok: opTok}
	e.Condition = p.parseExpr()

	if p.cur.Kind != token.OP_THEN {
		p.errorf(p.cur, "expected :then after :if condition")
		return e
	}
	p.advance() // consume :then

	// :then% means side-effect (subject passes through unchanged).
	if p.cur.Kind == token.PERCENT {
		e.SideEffect = true
		p.advance()
	}
	e.Then = p.parseExpr()

	if p.cur.Kind == token.OP_ELSE {
		p.advance() // consume :else
		e.Else = p.parseExpr()
	}
	return e
}

// parseWhileExpression parses the :while cond :then body group.
func (p *Parser) parseWhileExpression(opTok token.Token) *ast.WhileExpression {
	e := &ast.WhileExpression{Tok: opTok}
	e.Condition = p.parseExpr()

	if p.cur.Kind != token.OP_THEN {
		p.errorf(p.cur, "expected :then after :while condition")
		return e
	}
	p.advance() // consume :then

	if p.cur.Kind == token.PERCENT {
		e.SideEffect = true
		p.advance()
	}
	e.Body = p.parseExpr()
	return e
}

// parseCatchNamedExpression parses :catch [$alias] body.
func (p *Parser) parseCatchNamedExpression(opTok token.Token) *ast.CatchNamedExpression {
	e := &ast.CatchNamedExpression{Tok: opTok}
	if p.cur.Kind == token.DOLLAR {
		p.advance() // consume $
		if p.cur.Kind == token.IDENT {
			e.Alias = p.cur.Lexeme
			p.advance()
		}
	}
	e.Body = p.parseExpr()
	return e
}

// ── Expression parsing (Pratt) ─────────────────────────────────────────────────

// Precedence levels for infix operators.
const (
	precLowest  = iota
	precOr            // ||
	precAnd           // &&
	precEquality      // == !=
	precCompare       // < > <= >=
	precSum           // + -
	precProduct       // * /
)

func infixPrecedence(k token.Kind) int {
	switch k {
	case token.OROR:
		return precOr
	case token.ANDAND:
		return precAnd
	case token.EQEQ, token.NEQ:
		return precEquality
	case token.LT, token.GT, token.LTEQ, token.GTEQ:
		return precCompare
	case token.PLUS, token.MINUS:
		return precSum
	case token.STAR, token.SLASH, token.PERCENT:
		return precProduct
	}
	return precLowest
}

// parseExpr parses a full expression using Pratt precedence climbing.
func (p *Parser) parseExpr() ast.Expression {
	return p.parseInfix(precLowest)
}

func (p *Parser) parseInfix(minPrec int) ast.Expression {
	left := p.parseUnary()
	for {
		prec := infixPrecedence(p.cur.Kind)
		if prec <= minPrec {
			break
		}
		op := p.cur
		p.advance()
		right := p.parseInfix(prec)
		left = &ast.BinaryExpression{Left: left, Op: op, Right: right}
	}
	return left
}

func (p *Parser) parseUnary() ast.Expression {
	switch p.cur.Kind {
	case token.MINUS, token.BANG:
		op := p.cur
		p.advance()
		operand := p.parseUnary()
		return &ast.UnaryExpression{Op: op, Operand: operand}
	}
	return p.parsePrimary()
}

// parsePrimary parses the highest-precedence expression forms.
func (p *Parser) parsePrimary() ast.Expression {
	switch p.cur.Kind {
	case token.INT:
		return p.parseIntLiteral()
	case token.FLOAT:
		return p.parseFloatLiteral()
	case token.STRING:
		return p.parseStringLiteral()
	case token.KW_TRUE:
		tok := p.cur
		p.advance()
		return &ast.BoolLiteral{Tok: tok, Value: true}
	case token.KW_FALSE:
		tok := p.cur
		p.advance()
		return &ast.BoolLiteral{Tok: tok, Value: false}
	case token.LBRACKET:
		return p.parseArrayLiteral()
	case token.DOLLAR:
		return p.parseSubjectRef()
	case token.HASH:
		ref := p.parseTraitRef()
		return &ref
	case token.LPAREN:
		return p.parseGroupedOrHeadless()
	case token.IDENT:
		return p.parseCallOrIdent()
	case token.KW_MATCH:
		return p.parseMatchExpression()
	default:
		p.errorf(p.cur, "unexpected token %v in expression", p.cur.Kind)
		tok := p.cur
		p.advance()
		return &ast.Identifier{Tok: tok, Name: tok.Lexeme}
	}
}

// parseGroupedOrHeadless distinguishes (expr), (x -> expr), and (:: steps)
func (p *Parser) parseGroupedOrHeadless() ast.Expression {
	tok := p.cur
	p.advance() // consume '('

	// Headless pipeline: (:: steps)
	if p.cur.Kind.IsBackboneOp() {
		var steps []ast.PipelineStep
		for p.cur.Kind.IsBackboneOp() {
			steps = append(steps, p.parsePipelineStep())
		}
		p.expect(token.RPAREN)
		return &ast.HeadlessPipeline{Tok: tok, Steps: steps}
	}

	// Lambda: (x -> expr)
	if p.cur.Kind == token.IDENT && p.peek.Kind == token.ARROW {
		param := p.cur.Lexeme
		p.advance() // consume ident
		p.advance() // consume ->
		body := p.parseExpr()
		p.expect(token.RPAREN)
		return &ast.LambdaExpression{Tok: tok, Param: param, Body: body}
	}

	expr := p.parseExpr()
	p.expect(token.RPAREN)
	return &ast.GroupedExpr{Tok: tok, Expr: expr}
}

// parseCallOrIdent parses an identifier, optionally followed by a call.
// Handles dotted paths like module.name and qualified trait paths.
func (p *Parser) parseCallOrIdent() ast.Expression {
	if p.cur.Kind != token.IDENT {
		p.errorf(p.cur, "expected identifier")
		tok := p.cur
		p.advance()
		return &ast.Identifier{Tok: tok, Name: tok.Lexeme}
	}

	tok := p.cur
	parts := []string{p.cur.Lexeme}
	p.advance()

	// Collect dotted path: a.b.c
	for p.cur.Kind == token.DOT && p.peek.Kind == token.IDENT {
		p.advance() // consume '.'
		parts = append(parts, p.cur.Lexeme)
		p.advance()
	}

	var callee ast.Expression
	if len(parts) == 1 {
		callee = &ast.Identifier{Tok: tok, Name: parts[0]}
	} else {
		callee = &ast.ScopedIdent{Tok: tok, Parts: parts}
	}

	// Call expression: callee<TypeArgs>(Args)
	if p.cur.Kind == token.LPAREN || (p.cur.Kind == token.LT && isTypeArgStart(p.peek.Kind)) {
		return p.parseCallWith(callee)
	}
	return callee
}

// parseCallWith finishes parsing a call given an already-parsed callee.
func (p *Parser) parseCallWith(callee ast.Expression) *ast.CallExpression {
	call := &ast.CallExpression{Tok: callee.Pos(), Callee: callee}

	if p.cur.Kind == token.LT {
		call.TypeArgs = p.parseTypeArgList()
	}

	p.expect(token.LPAREN)
	call.Args = p.parseArgList()
	p.expect(token.RPAREN)
	return call
}

// parseArgList parses the argument list inside ( ).
func (p *Parser) parseArgList() []ast.Argument {
	var args []ast.Argument
	for p.cur.Kind != token.RPAREN && p.cur.Kind != token.EOF {
		args = append(args, p.parseArgument())
		if p.cur.Kind == token.COMMA {
			p.advance()
		}
	}
	return args
}

func (p *Parser) parseArgument() ast.Argument {
	// Subject placeholder: $
	if p.cur.Kind == token.DOLLAR && p.peek.Kind != token.IDENT {
		p.advance()
		return ast.Argument{IsSubject: true}
	}
	// Named argument: name = expr  (two-token look-ahead)
	if p.cur.Kind == token.IDENT && p.peek.Kind == token.EQUAL {
		name := p.cur.Lexeme
		p.advance() // consume name
		p.advance() // consume '='
		return ast.Argument{Name: name, Value: p.parseExpr()}
	}
	return ast.Argument{Value: p.parseExpr()}
}

// parseArgExprs is used for comptime argument lists (no named / subject forms).
func (p *Parser) parseArgExprs(until token.Kind) []ast.Expression {
	var exprs []ast.Expression
	for p.cur.Kind != until && p.cur.Kind != token.EOF {
		exprs = append(exprs, p.parseExpr())
		if p.cur.Kind == token.COMMA {
			p.advance()
		}
	}
	return exprs
}

// parseTypeArgList parses <Type, Type, ...>
func (p *Parser) parseTypeArgList() []ast.TypeExpr {
	p.advance() // consume '<'
	var args []ast.TypeExpr
	for p.cur.Kind != token.GT && p.cur.Kind != token.EOF {
		args = append(args, p.parseTypeExpr())
		if p.cur.Kind == token.COMMA {
			p.advance()
		}
	}
	p.expect(token.GT)
	return args
}

// parseOperatorApplication parses an arithmetic operator and its right operand
// for use as a pipeline step: :: * factor  → subject * factor
func (p *Parser) parseOperatorApplication() *ast.OperatorApplication {
	op := p.cur
	p.advance()
	right := p.parseExpr()
	return &ast.OperatorApplication{Op: op, Right: right}
}

// ── Match expression ───────────────────────────────────────────────────────────

func (p *Parser) parseMatchExpression() *ast.MatchExpression {
	m := &ast.MatchExpression{Tok: p.cur}
	p.advance() // consume 'match'
	p.expect(token.LPAREN)
	for p.cur.Kind != token.RPAREN && p.cur.Kind != token.EOF {
		arm := p.parseMatchArm()
		m.Arms = append(m.Arms, arm)
	}
	p.expect(token.RPAREN)
	return m
}

func (p *Parser) parseMatchArm() ast.MatchArm {
	pattern := p.parseExpr()
	arm := ast.MatchArm{Pattern: pattern}

	// Arm body: one or more :: steps
	for p.cur.Kind == token.OP_NEXT {
		arm.Steps = append(arm.Steps, p.parsePipelineStep())
	}
	return arm
}

// ── Literal parsers ────────────────────────────────────────────────────────────

func (p *Parser) parseIntLiteral() *ast.IntLiteral {
	tok := p.cur
	p.advance()
	v, err := strconv.ParseInt(tok.Lexeme, 0, 64)
	if err != nil {
		p.errorf(tok, "invalid integer: %s", tok.Lexeme)
	}
	return &ast.IntLiteral{Tok: tok, Value: v}
}

func (p *Parser) parseFloatLiteral() *ast.FloatLiteral {
	tok := p.cur
	p.advance()
	v, err := strconv.ParseFloat(tok.Lexeme, 64)
	if err != nil {
		p.errorf(tok, "invalid float: %s", tok.Lexeme)
	}
	return &ast.FloatLiteral{Tok: tok, Value: v}
}

func (p *Parser) parseArrayLiteral() *ast.ArrayLiteral {
	tok := p.cur
	p.advance() // consume '['
	var elements []ast.Expression
	for p.cur.Kind != token.RBRACKET && p.cur.Kind != token.EOF {
		elements = append(elements, p.parseExpr())
		if p.cur.Kind == token.COMMA {
			p.advance()
		}
	}
	p.expect(token.RBRACKET)
	return &ast.ArrayLiteral{Tok: tok, Elements: elements}
}

func (p *Parser) parseStringLiteral() *ast.StringLiteral {
	tok := p.cur
	p.advance()
	return &ast.StringLiteral{Tok: tok, Value: tok.Lexeme}
}

func (p *Parser) parseSubjectRef() *ast.SubjectRef {
	tok := p.cur
	p.advance() // consume '$'
	ref := &ast.SubjectRef{Tok: tok}
	if p.cur.Kind == token.IDENT {
		ref.Alias = p.cur.Lexeme
		p.advance()
	}
	return ref
}

// ── Type expression parsers ────────────────────────────────────────────────────

// parseTypeExpr parses a type reference: Name, Name<T>, or #Trait.
func (p *Parser) parseTypeExpr() ast.TypeExpr {
	if p.cur.Kind == token.HASH {
		ref := p.parseTraitRef()
		return &ref
	}
	if p.cur.Kind != token.IDENT {
		p.errorf(p.cur, "expected type name")
		tok := p.cur
		p.advance()
		return &ast.NamedType{Tok: tok, Name: tok.Lexeme}
	}
	tok := p.cur
	name := tok.Lexeme
	p.advance()

	var params []ast.TypeExpr
	if p.cur.Kind == token.LT {
		p.advance() // consume '<'
		for p.cur.Kind != token.GT && p.cur.Kind != token.EOF {
			params = append(params, p.parseTypeExpr())
			if p.cur.Kind == token.COMMA {
				p.advance()
			}
		}
		p.expect(token.GT)
	}
	return &ast.NamedType{Tok: tok, Name: name, Params: params}
}

// parseTraitRef parses #Name.
func (p *Parser) parseTraitRef() ast.TraitRef {
	tok := p.cur
	p.advance() // consume '#'
	if p.cur.Kind != token.IDENT {
		p.errorf(p.cur, "expected trait name after '#'")
		return ast.TraitRef{Tok: tok}
	}
	name := p.cur.Lexeme
	p.advance()
	return ast.TraitRef{Tok: tok, Name: name}
}

// ── Cursor helpers ─────────────────────────────────────────────────────────────

func (p *Parser) advance() {
	p.cur = p.peek
	p.peek = p.l.Next()
}

// expect consumes a token of kind k, recording an error if the kind differs.
func (p *Parser) expect(k token.Kind) bool {
	if p.cur.Kind == k {
		p.advance()
		return true
	}
	p.errorf(p.cur, "expected %v, got %v", k, p.cur.Kind)
	return false
}

func (p *Parser) errorf(tok token.Token, format string, args ...any) {
	p.errors = append(p.errors, ast.ParseError{
		Tok:     tok,
		Message: fmt.Sprintf(format, args...),
	})
}

// ── Character classification helpers ──────────────────────────────────────────

func isArithOp(k token.Kind) bool {
	switch k {
	case token.PLUS, token.MINUS, token.STAR, token.SLASH, token.PERCENT:
		return true
	}
	return false
}

// isTypeArgStart returns true for tokens that can open a type argument list.
// Used to avoid ambiguity between the < comparison operator and generic <T>.
func isTypeArgStart(k token.Kind) bool {
	return k == token.IDENT || k == token.HASH
}

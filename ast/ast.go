// Package ast defines the Abstract Syntax Tree nodes for the Iris language.
package ast

import (
	"fmt"
	"iris-lang/token"
	"strings"
)

// ── Node interfaces ────────────────────────────────────────────────────────────

// Node is the common interface for all AST nodes.
type Node interface {
	Pos() token.Token // the opening token of this node
	String() string   // human-readable representation
}

// Statement is a top-level or block-level construct.
type Statement interface {
	Node
	stmtNode()
}

// Expression produces a value.
type Expression interface {
	Node
	exprNode()
}

// TypeExpr represents a type in a signature.
type TypeExpr interface {
	Node
	typeNode()
}

// ── Program ────────────────────────────────────────────────────────────────────

// Program is the root of every parse tree.
type Program struct {
	Stmts  []Statement
	Errors []ParseError
}

func (p *Program) Pos() token.Token {
	if len(p.Stmts) > 0 {
		return p.Stmts[0].Pos()
	}
	return token.Token{}
}
func (p *Program) String() string {
	var sb strings.Builder
	for _, s := range p.Stmts {
		sb.WriteString(s.String())
		sb.WriteRune('\n')
	}
	return sb.String()
}

// ParseError captures a parse failure with location.
type ParseError struct {
	Tok     token.Token
	Message string
}

func (e ParseError) Error() string {
	return fmt.Sprintf("%d:%d: %s", e.Tok.Line, e.Tok.Col, e.Message)
}

// ── Statements ─────────────────────────────────────────────────────────────────

// ComptimeDirective is a compile-time annotation: !name(args)
type ComptimeDirective struct {
	Tok  token.Token // '!'
	Name string
	Args []Expression
}

func (d *ComptimeDirective) stmtNode()        {}
func (d *ComptimeDirective) Pos() token.Token  { return d.Tok }
func (d *ComptimeDirective) String() string {
	args := exprList(d.Args)
	return fmt.Sprintf("!%s(%s)", d.Name, args)
}

// UseStatement imports a module path: use std::*
type UseStatement struct {
	Tok  token.Token // 'use'
	Path string
}

func (u *UseStatement) stmtNode()        {}
func (u *UseStatement) Pos() token.Token  { return u.Tok }
func (u *UseStatement) String() string    { return "use " + u.Path }

// LetStatement binds a name to a pipeline value: let name =: pipeline
type LetStatement struct {
	Tok   token.Token // 'let'
	Name  string
	Value *Pipeline
}

func (l *LetStatement) stmtNode()        {}
func (l *LetStatement) Pos() token.Token  { return l.Tok }
func (l *LetStatement) String() string {
	if l.Value == nil {
		return "let " + l.Name
	}
	var sb strings.Builder
	sb.WriteString("let ")
	sb.WriteString(l.Name)
	if l.Value.Subject != nil {
		sb.WriteString("\n= ")
		sb.WriteString(l.Value.Subject.String())
	}
	for _, step := range l.Value.Steps {
		sb.WriteString("\n")
		sb.WriteString(step.String())
	}
	return sb.String()
}

// FnDeclaration defines a function.
type FnDeclaration struct {
	Tok        token.Token // 'fn' (or 'export')
	Exported   bool
	Name       string
	TypeParams []TypeParam
	Params     []FnParam
	ReturnType TypeExpr // nil if no return type
	Body       FnBody
}

func (f *FnDeclaration) stmtNode()        {}
func (f *FnDeclaration) Pos() token.Token  { return f.Tok }
func (f *FnDeclaration) String() string {
	var sb strings.Builder
	if f.Exported {
		sb.WriteString("export ")
	}
	sb.WriteString("fn ")
	sb.WriteString(f.Name)
	if len(f.TypeParams) > 0 {
		sb.WriteString("<")
		for i, tp := range f.TypeParams {
			if i > 0 {
				sb.WriteString(", ")
			}
			sb.WriteString(tp.String())
		}
		sb.WriteString(">")
	}
	sb.WriteString("(")
	for i, p := range f.Params {
		if i > 0 {
			sb.WriteString(", ")
		}
		sb.WriteString(p.String())
	}
	sb.WriteString(")")
	if f.ReturnType != nil {
		sb.WriteString(" -> ")
		sb.WriteString(f.ReturnType.String())
	}
	if f.Body != nil {
		sb.WriteString(" ")
		sb.WriteString(f.Body.String())
	}
	return sb.String()
}

// TraitDeclaration defines a trait: trait #Name : #Parent { methods }
type TraitDeclaration struct {
	Tok     token.Token // 'trait'
	Name    string
	Parent  *TraitRef // nil if no parent
	Methods []*FnDeclaration
}

func (t *TraitDeclaration) stmtNode()        {}
func (t *TraitDeclaration) Pos() token.Token  { return t.Tok }
func (t *TraitDeclaration) String() string {
	var sb strings.Builder
	sb.WriteString("trait #")
	sb.WriteString(t.Name)
	if t.Parent != nil {
		sb.WriteString(" : ")
		sb.WriteString(t.Parent.String())
	}
	sb.WriteString(" {")
	for _, m := range t.Methods {
		sb.WriteString("\n  ")
		sb.WriteString(m.String())
	}
	sb.WriteString("\n}")
	return sb.String()
}

// ImplDeclaration implements a trait for a type: impl #Trait for Type { methods }
type ImplDeclaration struct {
	Tok     token.Token // 'impl'
	Trait   TraitRef
	ForType TypeExpr
	Methods []*FnDeclaration
}

func (i *ImplDeclaration) stmtNode()        {}
func (i *ImplDeclaration) Pos() token.Token  { return i.Tok }
func (i *ImplDeclaration) String() string {
	var sb strings.Builder
	sb.WriteString("impl ")
	sb.WriteString(i.Trait.String())
	sb.WriteString(" for ")
	sb.WriteString(i.ForType.String())
	sb.WriteString(" {")
	for _, m := range i.Methods {
		sb.WriteString("\n  ")
		sb.WriteString(m.String())
	}
	sb.WriteString("\n}")
	return sb.String()
}

// ExpressionStatement wraps a standalone expression (typically a pipeline).
type ExpressionStatement struct {
	Expr Expression
}

func (e *ExpressionStatement) stmtNode()        {}
func (e *ExpressionStatement) Pos() token.Token  { return e.Expr.Pos() }
func (e *ExpressionStatement) String() string    { return e.Expr.String() }

// ── Function body variants ─────────────────────────────────────────────────────

// FnBody is either a block { stmts } or an expression =: pipeline.
type FnBody interface {
	Node
	fnBodyNode()
}

// BlockBody is a procedural function body: { stmts }
type BlockBody struct {
	Tok   token.Token // '{'
	Stmts []Statement
}

func (b *BlockBody) fnBodyNode()       {}
func (b *BlockBody) Pos() token.Token  { return b.Tok }
func (b *BlockBody) String() string {
	if len(b.Stmts) == 0 {
		return "{}"
	}
	var sb strings.Builder
	sb.WriteString("{")
	for _, s := range b.Stmts {
		for _, line := range strings.Split(s.String(), "\n") {
			sb.WriteString("\n  ")
			sb.WriteString(line)
		}
	}
	sb.WriteString("\n}")
	return sb.String()
}

// ExprBody is an expression function body: =: pipeline
type ExprBody struct {
	Pipeline *Pipeline
}

func (e *ExprBody) fnBodyNode()       {}
func (e *ExprBody) Pos() token.Token  { return e.Pipeline.Pos() }
func (e *ExprBody) String() string {
	var sb strings.Builder
	if e.Pipeline.Subject != nil {
		sb.WriteString("=: ")
		sb.WriteString(e.Pipeline.Subject.String())
	}
	for _, step := range e.Pipeline.Steps {
		sb.WriteString(" ")
		sb.WriteString(step.String())
	}
	return sb.String()
}

// ── Type expressions ───────────────────────────────────────────────────────────

// NamedType is a concrete type reference, optionally generic: Name or Name<T>
type NamedType struct {
	Tok    token.Token
	Name   string
	Params []TypeExpr
}

func (n *NamedType) typeNode()        {}
func (n *NamedType) Pos() token.Token  { return n.Tok }
func (n *NamedType) String() string {
	if len(n.Params) == 0 {
		return n.Name
	}
	params := make([]string, len(n.Params))
	for i, p := range n.Params {
		params[i] = p.String()
	}
	return n.Name + "<" + strings.Join(params, ", ") + ">"
}

// TraitRef is a trait reference: #Name
type TraitRef struct {
	Tok  token.Token // '#'
	Name string
}

func (t *TraitRef) typeNode()        {}
func (t *TraitRef) exprNode()        {} // TraitRef appears in both type and expression positions
func (t *TraitRef) Pos() token.Token  { return t.Tok }
func (t *TraitRef) String() string    { return "#" + t.Name }

// TraitBoundType is a trait used as a type bound: #Text && #Serializable
type TraitBoundType struct {
	Bounds []TraitRef
}

func (t *TraitBoundType) typeNode()        {}
func (t *TraitBoundType) Pos() token.Token  {
	if len(t.Bounds) > 0 {
		return t.Bounds[0].Tok
	}
	return token.Token{}
}
func (t *TraitBoundType) String() string {
	parts := make([]string, len(t.Bounds))
	for i, b := range t.Bounds {
		parts[i] = b.String()
	}
	return strings.Join(parts, " && ")
}

// TypeParam is a generic type parameter with trait bounds: T: #Text && #Serializable
type TypeParam struct {
	Name   string
	Bounds []TraitRef
}

func (tp TypeParam) String() string {
	if len(tp.Bounds) == 0 {
		return tp.Name
	}
	bounds := make([]string, len(tp.Bounds))
	for i, b := range tp.Bounds {
		bounds[i] = b.String()
	}
	return tp.Name + ": " + strings.Join(bounds, " && ")
}

// FnParam is a function parameter: name: Type
type FnParam struct {
	Name string
	Type TypeExpr // nil if omitted
}

func (p FnParam) String() string {
	if p.Type == nil {
		return p.Name
	}
	return p.Name + ": " + p.Type.String()
}

// ── Expressions ────────────────────────────────────────────────────────────────

// Pipeline is the core Iris expression: subject op step op step ...
// Subject may be nil for headless pipelines: (:: trim() :: lower())
type Pipeline struct {
	Tok     token.Token // token of the subject, or the first op if headless
	Subject Expression  // nil for headless
	Steps   []PipelineStep
}

func (p *Pipeline) exprNode()        {}
func (p *Pipeline) Pos() token.Token  { return p.Tok }
func (p *Pipeline) String() string {
	var sb strings.Builder
	if p.Subject != nil {
		sb.WriteString(p.Subject.String())
	}
	for _, step := range p.Steps {
		sb.WriteString("\n")
		sb.WriteString(step.String())
	}
	return sb.String()
}

// PipelineStep is one op+expression pair in a pipeline.
type PipelineStep struct {
	Op   token.Token // the backbone operator token
	Expr Expression  // what follows the op
}

func (s PipelineStep) String() string {
	return s.Op.Lexeme + " " + s.Expr.String()
}

// CallExpression calls a function, possibly with type arguments: name<T>(args)
type CallExpression struct {
	Tok      token.Token // opening token of the callee
	Callee   Expression  // usually Identifier or ScopedIdent
	TypeArgs []TypeExpr
	Args     []Argument
}

func (c *CallExpression) exprNode()        {}
func (c *CallExpression) Pos() token.Token  { return c.Tok }
func (c *CallExpression) String() string {
	var sb strings.Builder
	sb.WriteString(c.Callee.String())
	if len(c.TypeArgs) > 0 {
		args := make([]string, len(c.TypeArgs))
		for i, a := range c.TypeArgs {
			args[i] = a.String()
		}
		sb.WriteString("<")
		sb.WriteString(strings.Join(args, ", "))
		sb.WriteString(">")
	}
	sb.WriteString("(")
	for i, a := range c.Args {
		if i > 0 {
			sb.WriteString(", ")
		}
		sb.WriteString(a.String())
	}
	sb.WriteString(")")
	return sb.String()
}

// Argument is one argument in a call, which may be a subject placeholder ($).
type Argument struct {
	IsSubject bool       // true if this is the bare '$' subject placeholder
	Name      string    // non-empty for named arguments: name = expr
	Value     Expression // nil only when IsSubject is true
}

func (a Argument) String() string {
	if a.IsSubject {
		return "$"
	}
	if a.Name != "" {
		return a.Name + " = " + a.Value.String()
	}
	return a.Value.String()
}

// MatchExpression is: match ( arm* )
type MatchExpression struct {
	Tok  token.Token // 'match'
	Arms []MatchArm
}

func (m *MatchExpression) exprNode()        {}
func (m *MatchExpression) Pos() token.Token  { return m.Tok }
func (m *MatchExpression) String() string {
	var sb strings.Builder
	sb.WriteString("match (\n")
	for _, arm := range m.Arms {
		sb.WriteString("  ")
		sb.WriteString(arm.String())
		sb.WriteRune('\n')
	}
	sb.WriteString(")")
	return sb.String()
}

// MatchArm is one branch: Pattern :: steps
type MatchArm struct {
	Pattern Expression
	Steps   []PipelineStep // one or more :: steps
}

func (a MatchArm) String() string {
	s := a.Pattern.String()
	for _, step := range a.Steps {
		s += " " + step.String()
	}
	return s
}

// BinaryExpression is an infix expression: left op right
type BinaryExpression struct {
	Left  Expression
	Op    token.Token
	Right Expression
}

func (b *BinaryExpression) exprNode()        {}
func (b *BinaryExpression) Pos() token.Token  { return b.Left.Pos() }
func (b *BinaryExpression) String() string {
	return b.Left.String() + " " + b.Op.Lexeme + " " + b.Right.String()
}

// OperatorApplication applies a binary operator within a pipeline step: :: * 2
// This lets the pipeline subject become the left operand.
type OperatorApplication struct {
	Op    token.Token // +, -, *, /
	Right Expression
}

func (o *OperatorApplication) exprNode()        {}
func (o *OperatorApplication) Pos() token.Token  { return o.Op }
func (o *OperatorApplication) String() string {
	return o.Op.Lexeme + " " + o.Right.String()
}

// Identifier is a simple name: foo
type Identifier struct {
	Tok  token.Token
	Name string
}

func (i *Identifier) exprNode()        {}
func (i *Identifier) Pos() token.Token  { return i.Tok }
func (i *Identifier) String() string    { return i.Name }

// ScopedIdent is a dot-separated path: module.name or std.#String.trim
type ScopedIdent struct {
	Tok   token.Token
	Parts []string
}

func (s *ScopedIdent) exprNode()        {}
func (s *ScopedIdent) Pos() token.Token  { return s.Tok }
func (s *ScopedIdent) String() string    { return strings.Join(s.Parts, ".") }

// SubjectRef is a subject placeholder or alias: $ or $name
type SubjectRef struct {
	Tok   token.Token // '$'
	Alias string      // empty for bare $
}

func (s *SubjectRef) exprNode()        {}
func (s *SubjectRef) Pos() token.Token  { return s.Tok }
func (s *SubjectRef) String() string {
	if s.Alias == "" {
		return "$"
	}
	return "$" + s.Alias
}

// IntLiteral is an integer constant: 42, 0xff
type IntLiteral struct {
	Tok   token.Token
	Value int64
}

func (i *IntLiteral) exprNode()        {}
func (i *IntLiteral) Pos() token.Token  { return i.Tok }
func (i *IntLiteral) String() string    { return i.Tok.Lexeme }

// FloatLiteral is a floating-point constant: 3.14
type FloatLiteral struct {
	Tok   token.Token
	Value float64
}

func (f *FloatLiteral) exprNode()        {}
func (f *FloatLiteral) Pos() token.Token  { return f.Tok }
func (f *FloatLiteral) String() string    { return f.Tok.Lexeme }

// StringLiteral is a string constant: "hello"
type StringLiteral struct {
	Tok   token.Token
	Value string
}

func (s *StringLiteral) exprNode()        {}
func (s *StringLiteral) Pos() token.Token  { return s.Tok }
func (s *StringLiteral) String() string    { return fmt.Sprintf("%q", s.Value) }

// BoolLiteral is a boolean constant: true or false
type BoolLiteral struct {
	Tok   token.Token
	Value bool
}

func (b *BoolLiteral) exprNode()        {}
func (b *BoolLiteral) Pos() token.Token  { return b.Tok }
func (b *BoolLiteral) String() string {
	if b.Value {
		return "true"
	}
	return "false"
}

// ArrayLiteral is an array expression: [expr, expr, ...]
type ArrayLiteral struct {
	Tok      token.Token // '['
	Elements []Expression
}

func (a *ArrayLiteral) exprNode()        {}
func (a *ArrayLiteral) Pos() token.Token  { return a.Tok }
func (a *ArrayLiteral) String() string {
	parts := make([]string, len(a.Elements))
	for i, e := range a.Elements {
		parts[i] = e.String()
	}
	return "[" + strings.Join(parts, ", ") + "]"
}

// UnaryExpression is a prefix expression: -expr or !expr
type UnaryExpression struct {
	Op      token.Token
	Operand Expression
}

func (u *UnaryExpression) exprNode()        {}
func (u *UnaryExpression) Pos() token.Token  { return u.Op }
func (u *UnaryExpression) String() string    { return u.Op.Lexeme + u.Operand.String() }

// GroupedExpr is a parenthesized expression: (expr)
type GroupedExpr struct {
	Tok  token.Token // '('
	Expr Expression
}

func (g *GroupedExpr) exprNode()        {}
func (g *GroupedExpr) Pos() token.Token  { return g.Tok }
func (g *GroupedExpr) String() string    { return "(" + g.Expr.String() + ")" }

// IfExpression groups the :if/:then/:else trio into a single step expression.
// SideEffect is true when :then% is used — the subject passes through unchanged.
type IfExpression struct {
	Tok        token.Token // ':if' token
	Condition  Expression
	Then       Expression
	Else       Expression // nil when no :else
	SideEffect bool       // true for :then%
}

func (e *IfExpression) exprNode()        {}
func (e *IfExpression) Pos() token.Token  { return e.Tok }
func (e *IfExpression) String() string {
	s := ":if " + e.Condition.String()
	then := ":then"
	if e.SideEffect {
		then = ":then%"
	}
	s += " " + then + " " + e.Then.String()
	if e.Else != nil {
		s += " :else " + e.Else.String()
	}
	return s
}

// WhileExpression groups the :while/:then pair into a single step expression.
type WhileExpression struct {
	Tok        token.Token // ':while' token
	Condition  Expression
	Body       Expression
	SideEffect bool // true for :then%
}

func (e *WhileExpression) exprNode()        {}
func (e *WhileExpression) Pos() token.Token  { return e.Tok }
func (e *WhileExpression) String() string {
	then := ":then"
	if e.SideEffect {
		then = ":then%"
	}
	return ":while " + e.Condition.String() + " " + then + " " + e.Body.String()
}

// CatchNamedExpression is :catch $alias body — binds the error value to alias.
type CatchNamedExpression struct {
	Tok   token.Token // ':catch' token
	Alias string      // empty if no $name
	Body  Expression
}

func (e *CatchNamedExpression) exprNode()        {}
func (e *CatchNamedExpression) Pos() token.Token  { return e.Tok }
func (e *CatchNamedExpression) String() string {
	if e.Alias == "" {
		return ":catch " + e.Body.String()
	}
	return ":catch $" + e.Alias + " " + e.Body.String()
}

// LambdaExpression is a single-argument lambda: (x -> expr)
// The body is an Expression only — no pipelines inside.
type LambdaExpression struct {
	Tok   token.Token // '('
	Param string
	Body  Expression
}

func (l *LambdaExpression) exprNode()        {}
func (l *LambdaExpression) Pos() token.Token  { return l.Tok }
func (l *LambdaExpression) String() string {
	return "(" + l.Param + " -> " + l.Body.String() + ")"
}

// HeadlessPipeline is a pipeline with no subject: (:: trim() :: lower())
// It represents a reusable Flow object.
type HeadlessPipeline struct {
	Tok   token.Token // '('
	Steps []PipelineStep
}

func (h *HeadlessPipeline) exprNode()        {}
func (h *HeadlessPipeline) Pos() token.Token  { return h.Tok }
func (h *HeadlessPipeline) String() string {
	var sb strings.Builder
	sb.WriteString("(")
	for _, s := range h.Steps {
		sb.WriteString("\n  ")
		sb.WriteString(s.String())
	}
	sb.WriteString("\n)")
	return sb.String()
}

// ── Utilities ──────────────────────────────────────────────────────────────────

func exprList(exprs []Expression) string {
	parts := make([]string, len(exprs))
	for i, e := range exprs {
		parts[i] = e.String()
	}
	return strings.Join(parts, ", ")
}

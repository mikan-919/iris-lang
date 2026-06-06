package ast_test

import (
	"iris-lang/ast"
	"iris-lang/token"
	"testing"
)

// tok is a minimal token builder for tests.
func tok(kind token.Kind, lexeme string) token.Token {
	return token.Token{Kind: kind, Lexeme: lexeme, Line: 1, Col: 1}
}

// ── BoolLiteral ───────────────────────────────────────────────────────────────

func TestBoolLiteralStringTrue(t *testing.T) {
	b := &ast.BoolLiteral{Tok: tok(token.KW_TRUE, "true"), Value: true}
	if got := b.String(); got != "true" {
		t.Errorf("got %q, want %q", got, "true")
	}
}

func TestBoolLiteralStringFalse(t *testing.T) {
	b := &ast.BoolLiteral{Tok: tok(token.KW_FALSE, "false"), Value: false}
	if got := b.String(); got != "false" {
		t.Errorf("got %q, want %q", got, "false")
	}
}

// ── ArrayLiteral ──────────────────────────────────────────────────────────────

func TestArrayLiteralStringEmpty(t *testing.T) {
	a := &ast.ArrayLiteral{Tok: tok(token.LBRACKET, "[")}
	if got := a.String(); got != "[]" {
		t.Errorf("got %q, want %q", got, "[]")
	}
}

func TestArrayLiteralStringMultiple(t *testing.T) {
	a := &ast.ArrayLiteral{
		Tok: tok(token.LBRACKET, "["),
		Elements: []ast.Expression{
			&ast.IntLiteral{Tok: tok(token.INT, "1"), Value: 1},
			&ast.IntLiteral{Tok: tok(token.INT, "2"), Value: 2},
		},
	}
	if got := a.String(); got != "[1, 2]" {
		t.Errorf("got %q, want %q", got, "[1, 2]")
	}
}

// ── UnaryExpression ───────────────────────────────────────────────────────────

func TestUnaryExpressionStringMinus(t *testing.T) {
	u := &ast.UnaryExpression{
		Op:      tok(token.MINUS, "-"),
		Operand: &ast.IntLiteral{Tok: tok(token.INT, "42"), Value: 42},
	}
	if got := u.String(); got != "-42" {
		t.Errorf("got %q, want %q", got, "-42")
	}
}

func TestUnaryExpressionStringNot(t *testing.T) {
	u := &ast.UnaryExpression{
		Op:      tok(token.BANG, "!"),
		Operand: &ast.Identifier{Tok: tok(token.IDENT, "flag"), Name: "flag"},
	}
	if got := u.String(); got != "!flag" {
		t.Errorf("got %q, want %q", got, "!flag")
	}
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

func TestPipelineStringSubjectOnly(t *testing.T) {
	p := &ast.Pipeline{
		Tok:     tok(token.IDENT, "text"),
		Subject: &ast.Identifier{Tok: tok(token.IDENT, "text"), Name: "text"},
	}
	if got := p.String(); got != "text" {
		t.Errorf("got %q, want %q", got, "text")
	}
}

func TestPipelineStringWithSteps(t *testing.T) {
	p := &ast.Pipeline{
		Tok:     tok(token.IDENT, "text"),
		Subject: &ast.Identifier{Tok: tok(token.IDENT, "text"), Name: "text"},
		Steps: []ast.PipelineStep{
			{
				Op:   tok(token.OP_NEXT, "::"),
				Expr: &ast.Identifier{Tok: tok(token.IDENT, "trim"), Name: "trim"},
			},
		},
	}
	want := "text\n:: trim"
	if got := p.String(); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// ── HeadlessPipeline ──────────────────────────────────────────────────────────

func TestHeadlessPipelineString(t *testing.T) {
	h := &ast.HeadlessPipeline{
		Tok: tok(token.LPAREN, "("),
		Steps: []ast.PipelineStep{
			{Op: tok(token.OP_NEXT, "::"), Expr: &ast.Identifier{Tok: tok(token.IDENT, "trim"), Name: "trim"}},
		},
	}
	want := "(\n  :: trim\n)"
	if got := h.String(); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// ── LetStatement ──────────────────────────────────────────────────────────────

func TestLetStatementStringNoValue(t *testing.T) {
	l := &ast.LetStatement{Tok: tok(token.KW_LET, "let"), Name: "x"}
	if got := l.String(); got != "let x" {
		t.Errorf("got %q, want %q", got, "let x")
	}
}

func TestLetStatementStringWithLiteral(t *testing.T) {
	l := &ast.LetStatement{
		Tok:  tok(token.KW_LET, "let"),
		Name: "x",
		Value: &ast.Pipeline{
			Tok:     tok(token.EQUAL, "="),
			Subject: &ast.IntLiteral{Tok: tok(token.INT, "42"), Value: 42},
		},
	}
	want := "let x\n= 42"
	if got := l.String(); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// ── TraitRef ──────────────────────────────────────────────────────────────────

func TestTraitRefString(t *testing.T) {
	tr := &ast.TraitRef{Tok: tok(token.HASH, "#"), Name: "Text"}
	if got := tr.String(); got != "#Text" {
		t.Errorf("got %q, want %q", got, "#Text")
	}
}

// ── TraitBoundType ────────────────────────────────────────────────────────────

func TestTraitBoundTypeString(t *testing.T) {
	tb := &ast.TraitBoundType{
		Bounds: []ast.TraitRef{
			{Tok: tok(token.HASH, "#"), Name: "Text"},
			{Tok: tok(token.HASH, "#"), Name: "Serializable"},
		},
	}
	want := "#Text && #Serializable"
	if got := tb.String(); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// ── TypeParam ─────────────────────────────────────────────────────────────────

func TestTypeParamStringNoBounds(t *testing.T) {
	tp := ast.TypeParam{Name: "T"}
	if got := tp.String(); got != "T" {
		t.Errorf("got %q, want %q", got, "T")
	}
}

func TestTypeParamStringWithBounds(t *testing.T) {
	tp := ast.TypeParam{
		Name: "T",
		Bounds: []ast.TraitRef{
			{Tok: tok(token.HASH, "#"), Name: "Text"},
		},
	}
	want := "T: #Text"
	if got := tp.String(); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// ── NamedType ─────────────────────────────────────────────────────────────────

func TestNamedTypeStringSimple(t *testing.T) {
	n := &ast.NamedType{Tok: tok(token.IDENT, "Int"), Name: "Int"}
	if got := n.String(); got != "Int" {
		t.Errorf("got %q, want %q", got, "Int")
	}
}

func TestNamedTypeStringGeneric(t *testing.T) {
	n := &ast.NamedType{
		Tok:  tok(token.IDENT, "List"),
		Name: "List",
		Params: []ast.TypeExpr{
			&ast.NamedType{Tok: tok(token.IDENT, "Int"), Name: "Int"},
		},
	}
	want := "List<Int>"
	if got := n.String(); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

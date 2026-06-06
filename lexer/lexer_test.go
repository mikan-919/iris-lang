package lexer_test

import (
	"iris-lang/lexer"
	"iris-lang/token"
	"testing"
)

// tok is a test helper to build an expected token (ignoring position).
func tok(kind token.Kind, lexeme string) token.Token {
	return token.Token{Kind: kind, Lexeme: lexeme}
}

// lex runs the lexer on src and returns all non-EOF tokens.
func lex(src string) []token.Token {
	l := lexer.New(src)
	var ts []token.Token
	for {
		t := l.Next()
		if t.Kind == token.EOF {
			break
		}
		ts = append(ts, t)
	}
	return ts
}

func assertTokens(t *testing.T, src string, want []token.Token) {
	t.Helper()
	got := lex(src)
	if len(got) != len(want) {
		t.Fatalf("token count: got %d, want %d\nsrc: %q\ngot:  %v\nwant: %v",
			len(got), len(want), src, got, want)
	}
	for i, g := range got {
		w := want[i]
		if g.Kind != w.Kind || g.Lexeme != w.Lexeme {
			t.Errorf("token[%d]: got %v %q, want %v %q", i, g.Kind, g.Lexeme, w.Kind, w.Lexeme)
		}
	}
}

// ── Single-token tests ────────────────────────────────────────────────────────

func TestBackboneOperators(t *testing.T) {
	cases := []struct {
		src  string
		kind token.Kind
	}{
		{"::", token.OP_NEXT},
		{":or", token.OP_OR},
		{":try", token.OP_TRY},
		{":tag", token.OP_TAG},
		{":join", token.OP_JOIN},
		{":await", token.OP_AWAIT},
	}
	for _, c := range cases {
		t.Run(c.src, func(t *testing.T) {
			ts := lex(c.src)
			if len(ts) != 1 || ts[0].Kind != c.kind || ts[0].Lexeme != c.src {
				t.Errorf("got %v, want %v %q", ts, c.kind, c.src)
			}
		})
	}
}

func TestArithmeticOperators(t *testing.T) {
	assertTokens(t, "+ - * / %", []token.Token{
		tok(token.PLUS, "+"),
		tok(token.MINUS, "-"),
		tok(token.STAR, "*"),
		tok(token.SLASH, "/"),
		tok(token.PERCENT, "%"),
	})
}

func TestComparisonOperators(t *testing.T) {
	assertTokens(t, "== != < > <= >=", []token.Token{
		tok(token.EQEQ, "=="),
		tok(token.NEQ, "!="),
		tok(token.LT, "<"),
		tok(token.GT, ">"),
		tok(token.LTEQ, "<="),
		tok(token.GTEQ, ">="),
	})
}

func TestLogicalOperators(t *testing.T) {
	assertTokens(t, "&& ||", []token.Token{
		tok(token.ANDAND, "&&"),
		tok(token.OROR, "||"),
	})
}

func TestArrowAndEqual(t *testing.T) {
	assertTokens(t, "-> =", []token.Token{
		tok(token.ARROW, "->"),
		tok(token.EQUAL, "="),
	})
}

func TestSigils(t *testing.T) {
	assertTokens(t, "# $ ! @ | & ~ ^ ?", []token.Token{
		tok(token.HASH, "#"),
		tok(token.DOLLAR, "$"),
		tok(token.BANG, "!"),
		tok(token.AT, "@"),
		tok(token.PIPE, "|"),
		tok(token.AMP, "&"),
		tok(token.TILDE, "~"),
		tok(token.CARET, "^"),
		tok(token.QUEST, "?"),
	})
}

func TestDelimiters(t *testing.T) {
	assertTokens(t, "( ) { } [ ] , . : ;", []token.Token{
		tok(token.LPAREN, "("),
		tok(token.RPAREN, ")"),
		tok(token.LBRACE, "{"),
		tok(token.RBRACE, "}"),
		tok(token.LBRACKET, "["),
		tok(token.RBRACKET, "]"),
		tok(token.COMMA, ","),
		tok(token.DOT, "."),
		tok(token.COLON, ":"),
		tok(token.SEMICOLON, ";"),
	})
}

// ── Identifiers and keywords ──────────────────────────────────────────────────

func TestIdentifiers(t *testing.T) {
	cases := []string{"foo", "_bar", "myVar", "camelCase", "UPPER", "_123"}
	for _, id := range cases {
		t.Run(id, func(t *testing.T) {
			ts := lex(id)
			if len(ts) != 1 || ts[0].Kind != token.IDENT || ts[0].Lexeme != id {
				t.Errorf("got %v, want IDENT %q", ts, id)
			}
		})
	}
}

func TestKeywords(t *testing.T) {
	cases := []struct {
		src  string
		kind token.Kind
	}{
		{"let", token.KW_LET},
		{"fn", token.KW_FN},
		{"type", token.KW_TYPE},
		{"trait", token.KW_TRAIT},
		{"impl", token.KW_IMPL},
		{"use", token.KW_USE},
		{"for", token.KW_FOR},
		{"export", token.KW_EXPORT},
		{"match", token.KW_MATCH},
	}
	for _, c := range cases {
		t.Run(c.src, func(t *testing.T) {
			ts := lex(c.src)
			if len(ts) != 1 || ts[0].Kind != c.kind {
				t.Errorf("got %v, want %v", ts, c.kind)
			}
		})
	}
}

// ── Literals ──────────────────────────────────────────────────────────────────

func TestIntegerLiterals(t *testing.T) {
	cases := []string{"0", "42", "1000", "0xff", "0XAB"}
	for _, lit := range cases {
		t.Run(lit, func(t *testing.T) {
			ts := lex(lit)
			if len(ts) != 1 || ts[0].Kind != token.INT || ts[0].Lexeme != lit {
				t.Errorf("got %v, want INT %q", ts, lit)
			}
		})
	}
}

func TestFloatLiterals(t *testing.T) {
	cases := []string{"3.14", "0.5", "100.0"}
	for _, lit := range cases {
		t.Run(lit, func(t *testing.T) {
			ts := lex(lit)
			if len(ts) != 1 || ts[0].Kind != token.FLOAT || ts[0].Lexeme != lit {
				t.Errorf("got %v, want FLOAT %q", ts, lit)
			}
		})
	}
}

func TestStringLiterals(t *testing.T) {
	cases := []struct {
		src  string
		want string
	}{
		{`"hello"`, "hello"},
		{`"with spaces"`, "with spaces"},
		{`"escape\nnewline"`, "escape\nnewline"},
		{`"tab\there"`, "tab\there"},
		{`'single'`, "single"},
	}
	for _, c := range cases {
		t.Run(c.src, func(t *testing.T) {
			ts := lex(c.src)
			if len(ts) != 1 || ts[0].Kind != token.STRING || ts[0].Lexeme != c.want {
				t.Errorf("got %v, want STRING %q", ts, c.want)
			}
		})
	}
}

// ── Comments ──────────────────────────────────────────────────────────────────

func TestLineCommentSkipped(t *testing.T) {
	ts := lex("foo // this is a comment\nbar")
	assertTokens(t, "foo // this is a comment\nbar", []token.Token{
		tok(token.IDENT, "foo"),
		tok(token.IDENT, "bar"),
	})
	_ = ts
}

func TestDocCommentSkipped(t *testing.T) {
	assertTokens(t, "/// doc\nfn", []token.Token{
		tok(token.KW_FN, "fn"),
	})
}

func TestBlockCommentSkipped(t *testing.T) {
	assertTokens(t, "a /* skip this */ b", []token.Token{
		tok(token.IDENT, "a"),
		tok(token.IDENT, "b"),
	})
}

func TestBlockCommentMultiline(t *testing.T) {
	src := "x /* line1\nline2\nline3 */ y"
	assertTokens(t, src, []token.Token{
		tok(token.IDENT, "x"),
		tok(token.IDENT, "y"),
	})
}

// ── Position tracking ─────────────────────────────────────────────────────────

func TestPositionTracking(t *testing.T) {
	ts := lex("foo\nbar")
	if ts[0].Line != 1 || ts[0].Col != 1 {
		t.Errorf("foo: want 1:1, got %d:%d", ts[0].Line, ts[0].Col)
	}
	if ts[1].Line != 2 || ts[1].Col != 1 {
		t.Errorf("bar: want 2:1, got %d:%d", ts[1].Line, ts[1].Col)
	}
}

func TestColonStandaloneVsOp(t *testing.T) {
	assertTokens(t, ": ::", []token.Token{
		tok(token.COLON, ":"),
		tok(token.OP_NEXT, "::"),
	})
}

// ── Iris-specific patterns ────────────────────────────────────────────────────

func TestPipelineSequence(t *testing.T) {
	src := "text :: trim() :: split(\",\")"
	assertTokens(t, src, []token.Token{
		tok(token.IDENT, "text"),
		tok(token.OP_NEXT, "::"),
		tok(token.IDENT, "trim"),
		tok(token.LPAREN, "("),
		tok(token.RPAREN, ")"),
		tok(token.OP_NEXT, "::"),
		tok(token.IDENT, "split"),
		tok(token.LPAREN, "("),
		tok(token.STRING, ","),
		tok(token.RPAREN, ")"),
	})
}

func TestComptimeDirective(t *testing.T) {
	src := "!require(#Readable)"
	assertTokens(t, src, []token.Token{
		tok(token.BANG, "!"),
		tok(token.IDENT, "require"),
		tok(token.LPAREN, "("),
		tok(token.HASH, "#"),
		tok(token.IDENT, "Readable"),
		tok(token.RPAREN, ")"),
	})
}

func TestSubjectRef(t *testing.T) {
	assertTokens(t, "$ $name", []token.Token{
		tok(token.DOLLAR, "$"),
		tok(token.DOLLAR, "$"),
		tok(token.IDENT, "name"),
	})
}

func TestNextOp(t *testing.T) {
	src := "value :: next()"
	assertTokens(t, src, []token.Token{
		tok(token.IDENT, "value"),
		tok(token.OP_NEXT, "::"),
		tok(token.IDENT, "next"),
		tok(token.LPAREN, "("),
		tok(token.RPAREN, ")"),
	})
}

func TestBooleanKeywords(t *testing.T) {
	assertTokens(t, "true false", []token.Token{
		tok(token.KW_TRUE, "true"),
		tok(token.KW_FALSE, "false"),
	})
}

func TestAllBackboneOpsInSequence(t *testing.T) {
	src := ":: :or :try :tag :join :await"
	assertTokens(t, src, []token.Token{
		tok(token.OP_NEXT, "::"),
		tok(token.OP_OR, ":or"),
		tok(token.OP_TRY, ":try"),
		tok(token.OP_TAG, ":tag"),
		tok(token.OP_JOIN, ":join"),
		tok(token.OP_AWAIT, ":await"),
	})
}

func TestColonKeywordOps(t *testing.T) {
	cases := []struct {
		src  string
		kind token.Kind
	}{
		{":if", token.OP_IF},
		{":then", token.OP_THEN},
		{":else", token.OP_ELSE},
		{":while", token.OP_WHILE},
		{":catch", token.OP_CATCH_NAMED},
		{":await", token.OP_AWAIT},
	}
	for _, c := range cases {
		t.Run(c.src, func(t *testing.T) {
			ts := lex(c.src)
			if len(ts) != 1 || ts[0].Kind != c.kind || ts[0].Lexeme != c.src {
				t.Errorf("got %v, want %v %q", ts, c.kind, c.src)
			}
		})
	}
}

func TestColonKeywordDistinctFromColon(t *testing.T) {
	// ':' alone should still be COLON
	assertTokens(t, ":", []token.Token{tok(token.COLON, ":")})
}

func TestColonKeywordInPipeline(t *testing.T) {
	src := "value :if cond :then foo :else bar"
	assertTokens(t, src, []token.Token{
		tok(token.IDENT, "value"),
		tok(token.OP_IF, ":if"),
		tok(token.IDENT, "cond"),
		tok(token.OP_THEN, ":then"),
		tok(token.IDENT, "foo"),
		tok(token.OP_ELSE, ":else"),
		tok(token.IDENT, "bar"),
	})
}

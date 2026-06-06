// Package lexer converts Iris source text into a stream of tokens.
package lexer

import (
	"iris-lang/token"
	"strings"
	"unicode"
)

// Lexer holds the state for scanning one Iris source file.
type Lexer struct {
	src  []rune
	pos  int // current read position
	line int
	col  int
}

// New creates a Lexer for the given source text.
func New(src string) *Lexer {
	return &Lexer{src: []rune(src), line: 1, col: 1}
}

// Next returns the next token, skipping whitespace and comments.
func (l *Lexer) Next() token.Token {
	l.skipWhitespaceAndComments()

	line, col := l.line, l.col
	if l.eof() {
		return l.make(token.EOF, "", line, col)
	}

	ch := l.advance()

	switch ch {
	case ':':
		return l.colonOp(line, col)
	case '=':
		if l.match('=') {
			return l.make(token.EQEQ, "==", line, col)
		}
		return l.make(token.EQUAL, "=", line, col)
	case '-':
		if l.match('>') {
			return l.make(token.ARROW, "->", line, col)
		}
		return l.make(token.MINUS, "-", line, col)
	case '!':
		if l.match('=') {
			return l.make(token.NEQ, "!=", line, col)
		}
		return l.make(token.BANG, "!", line, col)
	case '<':
		if l.match('=') {
			return l.make(token.LTEQ, "<=", line, col)
		}
		return l.make(token.LT, "<", line, col)
	case '>':
		if l.match('=') {
			return l.make(token.GTEQ, ">=", line, col)
		}
		return l.make(token.GT, ">", line, col)
	case '&':
		if l.match('&') {
			return l.make(token.ANDAND, "&&", line, col)
		}
		return l.make(token.AMP, "&", line, col)
	case '|':
		if l.match('|') {
			return l.make(token.OROR, "||", line, col)
		}
		return l.make(token.PIPE, "|", line, col)
	case '+':
		return l.make(token.PLUS, "+", line, col)
	case '*':
		return l.make(token.STAR, "*", line, col)
	case '/':
		return l.make(token.SLASH, "/", line, col)
	case '%':
		return l.make(token.PERCENT, "%", line, col)
	case '#':
		return l.make(token.HASH, "#", line, col)
	case '$':
		return l.make(token.DOLLAR, "$", line, col)
	case '@':
		return l.make(token.AT, "@", line, col)
	case '~':
		return l.make(token.TILDE, "~", line, col)
	case '^':
		return l.make(token.CARET, "^", line, col)
	case '?':
		return l.make(token.QUEST, "?", line, col)
	case '(':
		return l.make(token.LPAREN, "(", line, col)
	case ')':
		return l.make(token.RPAREN, ")", line, col)
	case '{':
		return l.make(token.LBRACE, "{", line, col)
	case '}':
		return l.make(token.RBRACE, "}", line, col)
	case '[':
		return l.make(token.LBRACKET, "[", line, col)
	case ']':
		return l.make(token.RBRACKET, "]", line, col)
	case ',':
		return l.make(token.COMMA, ",", line, col)
	case '.':
		return l.make(token.DOT, ".", line, col)
	case ';':
		return l.make(token.SEMICOLON, ";", line, col)
	case '"', '\'':
		return l.lexString(ch, line, col)
	default:
		if unicode.IsDigit(ch) {
			return l.lexNumber(ch, line, col)
		}
		if isIdentStart(ch) {
			return l.lexIdent(ch, line, col)
		}
		return l.make(token.ILLEGAL, string(ch), line, col)
	}
}

// AllTokens returns every token up to and including EOF.
func (l *Lexer) AllTokens() []token.Token {
	var ts []token.Token
	for {
		t := l.Next()
		ts = append(ts, t)
		if t.Kind == token.EOF {
			break
		}
	}
	return ts
}

// ── Lexing sub-routines ───────────────────────────────────────────────────────

// colonOp handles all colon-prefixed backbone operators.
// The leading ':' has already been consumed.
func (l *Lexer) colonOp(line, col int) token.Token {
	switch {
	case l.match(':'):
		return l.make(token.OP_NEXT, "::", line, col)
	default:
		if !l.eof() && isIdentStart(l.peek()) {
			return l.colonKeywordOp(line, col)
		}
		return l.make(token.COLON, ":", line, col)
	}
}

// colonKeywordOp lexes a :keyword backbone op (e.g. :if, :then, :else).
// The ':' has already been consumed; the next char is alphabetic.
func (l *Lexer) colonKeywordOp(line, col int) token.Token {
	var sb strings.Builder
	sb.WriteRune(':')
	for !l.eof() && isIdentContinue(l.peek()) {
		sb.WriteRune(l.advance())
	}
	lexeme := sb.String()
	kind := colonKeywordKind(lexeme)
	return l.make(kind, lexeme, line, col)
}

// colonKeywordKind maps a :keyword lexeme to its token kind.
func colonKeywordKind(lexeme string) token.Kind {
	switch lexeme {
	case ":if":
		return token.OP_IF
	case ":then":
		return token.OP_THEN
	case ":else":
		return token.OP_ELSE
	case ":while":
		return token.OP_WHILE
	case ":catch":
		return token.OP_CATCH_NAMED
	case ":await":
		return token.OP_AWAIT
	case ":or":
		return token.OP_OR
	case ":try":
		return token.OP_TRY
	case ":tag":
		return token.OP_TAG
	case ":join":
		return token.OP_JOIN
	default:
		return token.ILLEGAL
	}
}

// lexString lexes a quoted string literal (single or double quote).
// The opening quote has already been consumed.
func (l *Lexer) lexString(quote rune, line, col int) token.Token {
	var sb strings.Builder
	for !l.eof() {
		ch := l.advance()
		if ch == quote {
			break
		}
		if ch == '\\' && !l.eof() {
			switch l.advance() {
			case 'n':
				sb.WriteRune('\n')
			case 't':
				sb.WriteRune('\t')
			case 'r':
				sb.WriteRune('\r')
			case '\\':
				sb.WriteRune('\\')
			case '"':
				sb.WriteRune('"')
			case '\'':
				sb.WriteRune('\'')
			}
			continue
		}
		sb.WriteRune(ch)
	}
	return l.make(token.STRING, sb.String(), line, col)
}

// lexNumber lexes an integer or float literal.
// The first digit ch has already been consumed.
func (l *Lexer) lexNumber(ch rune, line, col int) token.Token {
	var sb strings.Builder
	sb.WriteRune(ch)

	// Hex literal: 0x...
	if ch == '0' && !l.eof() && (l.peek() == 'x' || l.peek() == 'X') {
		sb.WriteRune(l.advance())
		for !l.eof() && isHexDigit(l.peek()) {
			sb.WriteRune(l.advance())
		}
		return l.make(token.INT, sb.String(), line, col)
	}

	for !l.eof() && unicode.IsDigit(l.peek()) {
		sb.WriteRune(l.advance())
	}

	// Float: digits '.' digits
	if !l.eof() && l.peek() == '.' && l.pos+1 < len(l.src) && unicode.IsDigit(l.src[l.pos+1]) {
		sb.WriteRune(l.advance()) // '.'
		for !l.eof() && unicode.IsDigit(l.peek()) {
			sb.WriteRune(l.advance())
		}
		return l.make(token.FLOAT, sb.String(), line, col)
	}

	return l.make(token.INT, sb.String(), line, col)
}

// lexIdent lexes an identifier or keyword.
// The first rune ch has already been consumed.
func (l *Lexer) lexIdent(ch rune, line, col int) token.Token {
	var sb strings.Builder
	sb.WriteRune(ch)
	for !l.eof() && isIdentContinue(l.peek()) {
		sb.WriteRune(l.advance())
	}
	text := sb.String()
	return l.make(token.LookupIdent(text), text, line, col)
}

// ── Comment / whitespace skipping ─────────────────────────────────────────────

func (l *Lexer) skipWhitespaceAndComments() {
	for !l.eof() {
		ch := l.peek()
		switch {
		case ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n':
			l.advance()
		case ch == '/' && l.pos+1 < len(l.src) && l.src[l.pos+1] == '/':
			l.skipLineComment()
		case ch == '/' && l.pos+1 < len(l.src) && l.src[l.pos+1] == '*':
			l.skipBlockComment()
		default:
			return
		}
	}
}

func (l *Lexer) skipLineComment() {
	for !l.eof() && l.peek() != '\n' {
		l.advance()
	}
}

func (l *Lexer) skipBlockComment() {
	l.advance() // '/'
	l.advance() // '*'
	for !l.eof() {
		if l.peek() == '*' && l.pos+1 < len(l.src) && l.src[l.pos+1] == '/' {
			l.advance() // '*'
			l.advance() // '/'
			return
		}
		l.advance()
	}
}

// ── Low-level cursor operations ────────────────────────────────────────────────

func (l *Lexer) peek() rune {
	if l.eof() {
		return 0
	}
	return l.src[l.pos]
}

func (l *Lexer) advance() rune {
	ch := l.src[l.pos]
	l.pos++
	if ch == '\n' {
		l.line++
		l.col = 1
	} else {
		l.col++
	}
	return ch
}

// match consumes and returns true if the next rune equals want.
func (l *Lexer) match(want rune) bool {
	if l.eof() || l.peek() != want {
		return false
	}
	l.advance()
	return true
}

func (l *Lexer) eof() bool {
	return l.pos >= len(l.src)
}

func (l *Lexer) make(kind token.Kind, lexeme string, line, col int) token.Token {
	return token.Token{Kind: kind, Lexeme: lexeme, Line: line, Col: col}
}

// ── Character classification ───────────────────────────────────────────────────

func isIdentStart(ch rune) bool {
	return ch == '_' || unicode.IsLetter(ch)
}

func isIdentContinue(ch rune) bool {
	return ch == '_' || unicode.IsLetter(ch) || unicode.IsDigit(ch)
}

func isHexDigit(ch rune) bool {
	return (ch >= '0' && ch <= '9') ||
		(ch >= 'a' && ch <= 'f') ||
		(ch >= 'A' && ch <= 'F')
}

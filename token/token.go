package token

type TokenType int

const (
	EOF TokenType = iota

	IDENT
	NUMBER
	STRING

	LET

	COLON
	EQUAL
	DOUBLE_COLON
	HASH
	DOLLAR
	PERCENT

	ARROW
	LPAREN
	RPAREN
	LBRACE
	RBRACE

	COMMA
	DOT
)

type Token struct {
	Type    TokenType
	Literal string
	Pos     int
}

// Package token defines the lexical tokens of the Iris language.
package token

import "fmt"

// Kind identifies the category of a token.
type Kind int

const (
	EOF     Kind = iota
	ILLEGAL      // unexpected character

	IDENT  // identifier: foo, Bar, myVar
	INT    // integer literal: 42
	FLOAT  // float literal: 3.14
	STRING // string literal: "hello"

	// ── Keywords ────────────────────────────────────────────────────────────
	KW_LET    // let
	KW_FN     // fn
	KW_TYPE   // type
	KW_TRAIT  // trait
	KW_IMPL   // impl
	KW_USE    // use
	KW_FOR    // for
	KW_EXPORT // export
	KW_MATCH  // match
	KW_TRUE   // true
	KW_FALSE  // false

	// ── Backbone pipeline operators ──────────────────────────────────────────
	// All backbone ops are :keyword or :: form
	OP_NEXT        // ::     continue (sync)
	OP_IF          // :if    conditional branch
	OP_THEN        // :then  branch body (used by :if and :while)
	OP_ELSE        // :else  fallback branch
	OP_WHILE       // :while loop condition
	OP_CATCH_NAMED // :catch named-error catch
	OP_AWAIT       // :await async wait
	OP_OR          // :or    fallback value
	OP_TRY         // :try   propagate error (short-circuit)
	OP_TAG         // :tag   borrow-save snapshot
	OP_JOIN        // :join  tuple join (subject, tag)

	// ── Arithmetic / relational operators ───────────────────────────────────
	PLUS    // +
	MINUS   // -
	STAR    // *
	SLASH   // /
	PERCENT // %

	EQEQ   // ==
	NEQ    // !=
	LT     // <
	GT     // >
	LTEQ   // <=
	GTEQ   // >=
	ANDAND // &&
	OROR   // ||
	ARROW  // ->

	// ── Delimiters ───────────────────────────────────────────────────────────
	LPAREN   // (
	RPAREN   // )
	LBRACE   // {
	RBRACE   // }
	LBRACKET // [
	RBRACKET // ]

	// ── Punctuation ──────────────────────────────────────────────────────────
	COMMA     // ,
	DOT       // .
	COLON     // :
	SEMICOLON // ;
	EQUAL     // =  (standalone, not part of =:)

	// ── Iris-specific sigils ─────────────────────────────────────────────────
	HASH   // #  trait reference prefix
	DOLLAR // $  subject position / alias
	BANG   // !  comptime directive prefix
	AT     // @  decorator
	PIPE   // |  join branch separator
	AMP    // &
	TILDE  // ~
	CARET  // ^
	QUEST  // ?
)

var keywords = map[string]Kind{
	"let":    KW_LET,
	"fn":     KW_FN,
	"type":   KW_TYPE,
	"trait":  KW_TRAIT,
	"impl":   KW_IMPL,
	"use":    KW_USE,
	"for":    KW_FOR,
	"export": KW_EXPORT,
	"match":  KW_MATCH,
	"true":   KW_TRUE,
	"false":  KW_FALSE,
}

// LookupIdent returns the keyword Kind for s, or IDENT if s is not a keyword.
func LookupIdent(s string) Kind {
	if k, ok := keywords[s]; ok {
		return k
	}
	return IDENT
}

var kindName = map[Kind]string{
	EOF: "EOF", ILLEGAL: "ILLEGAL",
	IDENT: "IDENT", INT: "INT", FLOAT: "FLOAT", STRING: "STRING",
	KW_LET: "let", KW_FN: "fn", KW_TYPE: "type", KW_TRAIT: "trait",
	KW_IMPL: "impl", KW_USE: "use", KW_FOR: "for", KW_EXPORT: "export", KW_MATCH: "match",
	KW_TRUE: "true", KW_FALSE: "false",
	OP_NEXT: "::", OP_IF: ":if", OP_THEN: ":then", OP_ELSE: ":else",
	OP_WHILE: ":while", OP_CATCH_NAMED: ":catch", OP_AWAIT: ":await",
	OP_OR: ":or", OP_TRY: ":try", OP_TAG: ":tag", OP_JOIN: ":join",
	PLUS: "+", MINUS: "-", STAR: "*", SLASH: "/", PERCENT: "%",
	EQEQ: "==", NEQ: "!=", LT: "<", GT: ">", LTEQ: "<=", GTEQ: ">=",
	ANDAND: "&&", OROR: "||", ARROW: "->",
	LPAREN: "(", RPAREN: ")", LBRACE: "{", RBRACE: "}", LBRACKET: "[", RBRACKET: "]",
	COMMA: ",", DOT: ".", COLON: ":", SEMICOLON: ";", EQUAL: "=",
	HASH: "#", DOLLAR: "$", BANG: "!", AT: "@", PIPE: "|", AMP: "&",
	TILDE: "~", CARET: "^", QUEST: "?",
}

func (k Kind) String() string {
	if s, ok := kindName[k]; ok {
		return s
	}
	return fmt.Sprintf("Kind(%d)", int(k))
}

// IsBackboneOp reports whether k is a pipeline backbone operator.
func (k Kind) IsBackboneOp() bool {
	return k >= OP_NEXT && k <= OP_JOIN
}

// Token is a single lexical unit produced by the lexer.
type Token struct {
	Kind   Kind
	Lexeme string // raw text from source
	Line   int
	Col    int
}

func (t Token) String() string {
	return fmt.Sprintf("%v %q (%d:%d)", t.Kind, t.Lexeme, t.Line, t.Col)
}

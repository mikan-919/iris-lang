package typechecker

import (
	"fmt"
	"iris-lang/token"
)

// ErrorKind categorises a type or lifetime error.
type ErrorKind int

const (
	ErrTypeMismatch       ErrorKind = iota // generic mismatch
	ErrUndefinedVar                        // variable not in scope
	ErrArrayHetero                         // mixed element types in array literal
	ErrBinaryTypeMismatch                  // operands to binary op have incompatible types
	ErrUnaryTypeInvalid                    // operand type not valid for unary op
	ErrTraitBoundViolation                 // generic instantiation without required impl
	ErrImplMissing                         // impl block missing required trait methods
	ErrLifetimeEscape                      // tag used outside its defining pipeline
	ErrLifetimeDeadTag                     // :& join references an unknown tag
)

// TypeError is a single type or lifetime diagnostic.
type TypeError struct {
	Tok     token.Token
	Kind    ErrorKind
	Message string
}

func (e TypeError) Error() string {
	if e.Tok.Line > 0 {
		return fmt.Sprintf("%d:%d: %s", e.Tok.Line, e.Tok.Col, e.Message)
	}
	return e.Message
}

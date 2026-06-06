// Package eval is the Iris tree-walking interpreter.
package eval

import (
	"fmt"
	"iris-lang/ast"
	"strings"
)

// Value is the runtime representation of any Iris value.
type Value interface {
	Type() string
	String() string
}

// ── Scalar values ─────────────────────────────────────────────────────────────

type IntValue struct{ V int64 }

func (v *IntValue) Type() string   { return "Int" }
func (v *IntValue) String() string { return fmt.Sprintf("%d", v.V) }

type FloatValue struct{ V float64 }

func (v *FloatValue) Type() string   { return "Float" }
func (v *FloatValue) String() string { return fmt.Sprintf("%g", v.V) }

type StringValue struct{ V string }

func (v *StringValue) Type() string   { return "String" }
func (v *StringValue) String() string { return v.V }

type BoolValue struct{ V bool }

func (v *BoolValue) Type() string   { return "Bool" }
func (v *BoolValue) String() string { return fmt.Sprintf("%v", v.V) }

// ── Composite values ──────────────────────────────────────────────────────────

type ArrayValue struct{ Elements []Value }

func (v *ArrayValue) Type() string { return "Array" }
func (v *ArrayValue) String() string {
	parts := make([]string, len(v.Elements))
	for i, e := range v.Elements {
		parts[i] = e.String()
	}
	return "[" + strings.Join(parts, ", ") + "]"
}

type TupleValue struct{ Fields []Value }

func (v *TupleValue) Type() string { return "Tuple" }
func (v *TupleValue) String() string {
	parts := make([]string, len(v.Fields))
	for i, f := range v.Fields {
		parts[i] = f.String()
	}
	return "(" + strings.Join(parts, ", ") + ")"
}

// ── Special values ────────────────────────────────────────────────────────────

// NilValue is the unit value — returned when nothing else applies.
type NilValue struct{}

func (v *NilValue) Type() string   { return "()" }
func (v *NilValue) String() string { return "nil" }

// ErrorValue wraps a runtime error that propagates through :^ and is caught by :?.
type ErrorValue struct{ Message string }

func (v *ErrorValue) Type() string   { return "Error" }
func (v *ErrorValue) String() string { return "error(" + v.Message + ")" }

// FnValue is a user-defined function closure.
type FnValue struct {
	Decl *ast.FnDeclaration
	Env  *Env
}

func (v *FnValue) Type() string   { return "Fn" }
func (v *FnValue) String() string { return "fn(" + v.Decl.Name + ")" }

// FlowValue is a headless pipeline — a reusable sequence of steps.
type FlowValue struct {
	Steps []ast.PipelineStep
	Env   *Env
}

func (v *FlowValue) Type() string   { return "Flow" }
func (v *FlowValue) String() string { return "(::...)" }

// LambdaValue is a single-argument lambda: (x -> expr)
type LambdaValue struct {
	Param string
	Body  ast.Expression
	Env   *Env
}

func (v *LambdaValue) Type() string   { return "Lambda" }
func (v *LambdaValue) String() string { return "(" + v.Param + " -> ...)" }

// ── LetResult — the top-level result of evaluating a program ─────────────────

// LetResult is returned by Exec and gives access to all top-level let bindings.
type LetResult interface {
	Value
	Get(name string) Value
}

type letResult struct {
	bindings map[string]Value
	last     Value
}

func newLetResult() *letResult {
	return &letResult{bindings: make(map[string]Value)}
}

func (r *letResult) Type() string            { return "LetResult" }
func (r *letResult) String() string          { return r.last.String() }
func (r *letResult) Get(name string) Value   { return r.bindings[name] }
func (r *letResult) set(name string, v Value) { r.bindings[name] = v; r.last = v }

// ── Helpers ───────────────────────────────────────────────────────────────────

func isError(v Value) bool {
	_, ok := v.(*ErrorValue)
	return ok
}

func isFalsy(v Value) bool {
	switch val := v.(type) {
	case *NilValue:
		return true
	case *ErrorValue:
		return true
	case *BoolValue:
		return !val.V
	}
	return false
}

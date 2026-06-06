// Package typechecker implements type inference and lifetime analysis for Iris.
package typechecker

import "strings"

// Type is the interface all Iris types satisfy.
type Type interface {
	Equals(Type) bool
	String() string
}

// ── Primitive types ───────────────────────────────────────────────────────────

// PrimitiveType represents a built-in scalar: Int, Float, String, Bool, ().
type PrimitiveType struct{ Name string }

func (p *PrimitiveType) Equals(o Type) bool {
	q, ok := o.(*PrimitiveType)
	return ok && p.Name == q.Name
}
func (p *PrimitiveType) String() string { return p.Name }

// Predeclared primitives.
var (
	TypeInt     = &PrimitiveType{"Int"}
	TypeFloat   = &PrimitiveType{"Float"}
	TypeString  = &PrimitiveType{"String"}
	TypeBool    = &PrimitiveType{"Bool"}
	TypeUnit    = &PrimitiveType{"()"}
	TypeUnknown = &unknownType{}
)

// unknownType is a placeholder when the type cannot be determined.
// (e.g., call to a function that hasn't been declared)
type unknownType struct{}

func (u *unknownType) Equals(o Type) bool { _, ok := o.(*unknownType); return ok }
func (u *unknownType) String() string     { return "unknown" }

// NewUnknown returns the singleton unknown type.
func NewUnknown() Type { return TypeUnknown }

// ── Composite types ───────────────────────────────────────────────────────────

// ArrayType is a homogeneous list: [T].
type ArrayType struct{ Element Type }

func NewArrayType(elem Type) *ArrayType { return &ArrayType{elem} }
func (a *ArrayType) Equals(o Type) bool {
	b, ok := o.(*ArrayType)
	return ok && a.Element.Equals(b.Element)
}
func (a *ArrayType) String() string { return "[" + a.Element.String() + "]" }

// TupleType is the result of a :& join step: (T1, T2).
type TupleType struct{ Fields []Type }

func NewTupleType(fields ...Type) *TupleType { return &TupleType{fields} }
func (t *TupleType) Equals(o Type) bool {
	u, ok := o.(*TupleType)
	if !ok || len(t.Fields) != len(u.Fields) {
		return false
	}
	for i, f := range t.Fields {
		if !f.Equals(u.Fields[i]) {
			return false
		}
	}
	return true
}
func (t *TupleType) String() string {
	parts := make([]string, len(t.Fields))
	for i, f := range t.Fields {
		parts[i] = f.String()
	}
	return "(" + strings.Join(parts, ", ") + ")"
}

// NamedType is a concrete named type, optionally generic: Foo or Foo<T>.
type NamedType struct {
	Name   string
	Params []Type
}

func (n *NamedType) Equals(o Type) bool {
	m, ok := o.(*NamedType)
	if !ok || n.Name != m.Name || len(n.Params) != len(m.Params) {
		return false
	}
	for i, p := range n.Params {
		if !p.Equals(m.Params[i]) {
			return false
		}
	}
	return true
}
func (n *NamedType) String() string {
	if len(n.Params) == 0 {
		return n.Name
	}
	ps := make([]string, len(n.Params))
	for i, p := range n.Params {
		ps[i] = p.String()
	}
	return n.Name + "<" + strings.Join(ps, ", ") + ">"
}

// TraitType is a trait reference used as a type bound: #Text.
type TraitType struct{ Name string }

func (t *TraitType) Equals(o Type) bool {
	u, ok := o.(*TraitType)
	return ok && t.Name == u.Name
}
func (t *TraitType) String() string { return "#" + t.Name }

// TypeVar is a generic type parameter: T.
type TypeVar struct{ Name string }

func (v *TypeVar) Equals(o Type) bool {
	w, ok := o.(*TypeVar)
	return ok && v.Name == w.Name
}
func (v *TypeVar) String() string { return v.Name }

// FnType is a function signature: (T1, T2) -> R.
type FnType struct {
	Params []Type
	Return Type
}

func (f *FnType) Equals(o Type) bool {
	g, ok := o.(*FnType)
	if !ok || len(f.Params) != len(g.Params) {
		return false
	}
	for i, p := range f.Params {
		if !p.Equals(g.Params[i]) {
			return false
		}
	}
	return f.Return.Equals(g.Return)
}
func (f *FnType) String() string {
	ps := make([]string, len(f.Params))
	for i, p := range f.Params {
		ps[i] = p.String()
	}
	ret := "unknown"
	if f.Return != nil {
		ret = f.Return.String()
	}
	return "(" + strings.Join(ps, ", ") + ") -> " + ret
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// isNumeric reports whether t is Int or Float.
func isNumeric(t Type) bool {
	return t.Equals(TypeInt) || t.Equals(TypeFloat)
}

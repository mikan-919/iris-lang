package typechecker

// Env is a lexically-scoped type environment.
// It chains to a parent for nested scopes.
type Env struct {
	vars   map[string]Type    // variable name → type
	fns    map[string]*FnSig  // function name → signature
	traits map[string]*TraitDef
	impls  []ImplEntry
	parent *Env
}

// FnSig is the resolved signature of a function.
type FnSig struct {
	TypeParams []string    // generic parameter names
	Bounds     []BoundSet  // bounds[i] applies to TypeParams[i]
	Params     []ParamSig
	Return     Type        // nil means no return / unit
}

// ParamSig is one function parameter.
type ParamSig struct {
	Name string
	Type Type
}

// BoundSet is the set of trait bounds on a single type parameter.
type BoundSet []string // trait names

// TraitDef records the methods required by a trait declaration.
type TraitDef struct {
	Name    string
	Parent  string // parent trait name; empty if none
	Methods []string
}

// ImplEntry records that a concrete type implements a trait.
type ImplEntry struct {
	TraitName string
	ForType   string // concrete type name (e.g., "String", "Int")
	Methods   []string
}

// NewEnv creates a root environment with no parent.
func NewEnv() *Env {
	return &Env{
		vars:   make(map[string]Type),
		fns:    make(map[string]*FnSig),
		traits: make(map[string]*TraitDef),
	}
}

func (e *Env) child() *Env {
	return &Env{
		vars:   make(map[string]Type),
		fns:    make(map[string]*FnSig),
		traits: e.traits, // traits are global
		impls:  e.impls,  // impls are global
		parent: e,
	}
}

// setVar binds name to type in the current scope.
func (e *Env) setVar(name string, t Type) { e.vars[name] = t }

// getVar looks up a variable, walking parent scopes.
func (e *Env) getVar(name string) (Type, bool) {
	if t, ok := e.vars[name]; ok {
		return t, true
	}
	if e.parent != nil {
		return e.parent.getVar(name)
	}
	return nil, false
}

// setFn registers a function signature.
func (e *Env) setFn(name string, sig *FnSig) { e.fns[name] = sig }

// getFn looks up a function signature.
func (e *Env) getFn(name string) (*FnSig, bool) {
	if sig, ok := e.fns[name]; ok {
		return sig, true
	}
	if e.parent != nil {
		return e.parent.getFn(name)
	}
	return nil, false
}

// setTrait registers a trait definition.
func (e *Env) setTrait(name string, def *TraitDef) { e.traits[name] = def }

// getTrait looks up a trait definition.
func (e *Env) getTrait(name string) (*TraitDef, bool) {
	def, ok := e.traits[name]
	return def, ok
}

// addImpl records that forType implements traitName.
func (e *Env) addImpl(entry ImplEntry) { e.impls = append(e.impls, entry) }

// implements reports whether typeName implements traitName, considering trait
// inheritance (a child trait's impls satisfy parent trait requirements).
func (e *Env) implements(typeName, traitName string) bool {
	for _, imp := range e.impls {
		if imp.ForType == typeName && imp.TraitName == traitName {
			return true
		}
	}
	// Check via trait inheritance: if traitName has a parent, and typeName
	// implements the parent, that also satisfies the child's supertype.
	if def, ok := e.getTrait(traitName); ok && def.Parent != "" {
		return e.implements(typeName, def.Parent)
	}
	return false
}

// typeName extracts a simple string name from a Type for impl lookup.
func typeName(t Type) string {
	switch v := t.(type) {
	case *PrimitiveType:
		return v.Name
	case *NamedType:
		return v.Name
	default:
		return t.String()
	}
}

package eval

// Env is a lexically-scoped runtime environment.
type Env struct {
	vars   map[string]Value
	parent *Env
}

// NewEnv creates a root environment.
func NewEnv() *Env {
	return &Env{vars: make(map[string]Value)}
}

func (e *Env) child() *Env {
	return &Env{vars: make(map[string]Value), parent: e}
}

func (e *Env) set(name string, v Value) {
	e.vars[name] = v
}

func (e *Env) get(name string) (Value, bool) {
	if v, ok := e.vars[name]; ok {
		return v, true
	}
	if e.parent != nil {
		return e.parent.get(name)
	}
	return nil, false
}

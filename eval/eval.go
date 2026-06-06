package eval

import (
	"fmt"
	"iris-lang/ast"
	"iris-lang/token"
)

// Evaluator is the tree-walking interpreter.
type Evaluator struct {
	env  *Env
	tags map[string]Value // :> tag bindings for the current pipeline scope
}

// New creates a fresh Evaluator with an empty root environment.
func New() *Evaluator {
	return &Evaluator{env: NewEnv(), tags: make(map[string]Value)}
}

// Exec evaluates a full program and returns a LetResult that holds all
// top-level let bindings.  Expression statements' values are accessible
// as the "last" value on the result.
func (ev *Evaluator) Exec(prog *ast.Program) Value {
	result := newLetResult()

	// First pass: register all user-defined functions so forward refs work.
	for _, stmt := range prog.Stmts {
		if fn, ok := stmt.(*ast.FnDeclaration); ok {
			ev.env.set(fn.Name, &FnValue{Decl: fn, Env: ev.env})
		}
	}

	for _, stmt := range prog.Stmts {
		v := ev.evalStmt(stmt)
		if let, ok := stmt.(*ast.LetStatement); ok {
			result.set(let.Name, v)
		} else if v != nil {
			result.last = v
		}
	}

	if result.last == nil {
		result.last = &NilValue{}
	}
	return result
}

// ── Statement evaluation ──────────────────────────────────────────────────────

func (ev *Evaluator) evalStmt(stmt ast.Statement) Value {
	switch s := stmt.(type) {
	case *ast.LetStatement:
		return ev.evalLet(s)
	case *ast.FnDeclaration:
		return nil // already registered in Exec
	case *ast.ExpressionStatement:
		return ev.evalExpr(s.Expr)
	case *ast.ComptimeDirective:
		return nil
	case *ast.UseStatement:
		return nil
	default:
		return nil
	}
}

func (ev *Evaluator) evalLet(s *ast.LetStatement) Value {
	if s.Value == nil {
		return &NilValue{}
	}
	v := ev.evalPipeline(s.Value, &NilValue{})
	ev.env.set(s.Name, v)
	return v
}

// ── Pipeline execution ────────────────────────────────────────────────────────

// evalPipeline evaluates a pipeline, threading the subject through each step.
func (ev *Evaluator) evalPipeline(pipe *ast.Pipeline, hint Value) Value {
	// Save and restore the tag scope so nested pipelines don't clobber outer tags.
	savedTags := ev.tags
	ev.tags = copyTags(savedTags) // child scope sees parent tags
	defer func() { ev.tags = savedTags }()

	var subject Value
	if pipe.Subject != nil {
		subject = ev.evalExpr(pipe.Subject)
	} else {
		subject = hint
	}

	propagating := false // true after :^ fires with an error
	for _, step := range pipe.Steps {
		// In propagation mode (after :^ with an error), only :? and :| can
		// intercept the error; all other steps are skipped.
		if propagating {
			if step.Op.Kind == token.OP_CATCH_NAMED || step.Op.Kind == token.OP_OR {
				subject = ev.evalStep(step, subject)
				if !isError(subject) {
					propagating = false
				}
			}
			continue
		}
		subject = ev.evalStep(step, subject)
		if isError(subject) && step.Op.Kind == token.OP_TRY {
			propagating = true
		}
	}
	return subject
}

func (ev *Evaluator) evalStep(step ast.PipelineStep, subject Value) Value {
	switch step.Op.Kind {

	case token.OP_TAG: // :tag — borrow-save: bind tag, pass subject through
		if ident, ok := step.Expr.(*ast.Identifier); ok {
			ev.tags[ident.Name] = subject
		}
		return subject

	case token.OP_JOIN: // :join — tuple join: (subject, tag)
		if ident, ok := step.Expr.(*ast.Identifier); ok {
			tag, ok := ev.tags[ident.Name]
			if !ok {
				return &ErrorValue{"join: tag '" + ident.Name + "' not found"}
			}
			return &TupleValue{Fields: []Value{subject, tag}}
		}
		return subject

	case token.OP_OR: // :or — fallback when subject is falsy
		if !isFalsy(subject) {
			return subject
		}
		return ev.evalExpr(step.Expr)

	case token.OP_TRY: // :try — if subject is an error, short-circuit the pipeline
		return subject

	case token.OP_AWAIT: // :await — treat as sync for now
		return subject

	case token.OP_THEN, token.OP_ELSE: // consumed by :if/:while; should not appear standalone
		return subject

	case token.OP_IF, token.OP_WHILE: // composite keyword steps
		return ev.evalTransformStep(step, subject)

	default: // ::  and everything else
		return ev.evalTransformStep(step, subject)
	}
}

// evalTransformStep handles :: and other transform ops.
func (ev *Evaluator) evalTransformStep(step ast.PipelineStep, subject Value) Value {
	switch expr := step.Expr.(type) {
	case *ast.OperatorApplication:
		return ev.applyOperator(expr.Op, subject, ev.evalExpr(expr.Right))
	case *ast.MatchExpression:
		return ev.evalMatch(expr, subject)
	case *ast.IfExpression:
		return ev.evalIfStep(expr, subject)
	case *ast.WhileExpression:
		return ev.evalWhileStep(expr, subject)
	case *ast.CatchNamedExpression:
		return ev.evalCatchNamed(expr, subject)
	case *ast.CallExpression:
		return ev.evalCall(expr, subject)
	case *ast.Identifier:
		if expr.Name == "" {
			return subject // standalone :^ / :! / :await
		}
		// Bare identifier: look up as a function/flow and call with subject.
		if v, ok := ev.env.get(expr.Name); ok {
			return ev.applyValue(v, subject, nil)
		}
		return subject
	default:
		return ev.evalExpr(expr)
	}
}

// evalIfStep evaluates an :if cond :then body [:else fallback] step.
// evalCondition evaluates a condition expression against the current subject.
// If the condition is a LambdaValue, the subject is bound to the param.
func (ev *Evaluator) evalCondition(cond ast.Expression, subject Value) Value {
	val := ev.evalExprWithSubject(cond, subject)
	if lam, ok := val.(*LambdaValue); ok {
		child := lam.Env.child()
		child.set(lam.Param, subject)
		return (&Evaluator{env: child, tags: make(map[string]Value)}).evalExpr(lam.Body)
	}
	return val
}

func (ev *Evaluator) evalIfStep(e *ast.IfExpression, subject Value) Value {
	cond := ev.evalCondition(e.Condition, subject)
	var branch ast.Expression
	if !isFalsy(cond) {
		branch = e.Then
	} else if e.Else != nil {
		branch = e.Else
	} else {
		return subject // no :else and condition is falsy — pass through
	}
	result := ev.evalExprWithSubject(branch, subject)
	if e.SideEffect {
		return subject // :then% — don't replace subject
	}
	return result
}

// evalWhileStep evaluates a :while cond :then body step.
// The body is applied to the subject on each iteration as long as cond is truthy.
func (ev *Evaluator) evalWhileStep(e *ast.WhileExpression, subject Value) Value {
	const maxIter = 1_000_000 // safety limit against infinite loops
	for i := 0; i < maxIter; i++ {
		cond := ev.evalExprWithSubject(e.Condition, subject)
		if isFalsy(cond) {
			break
		}
		result := ev.evalExprWithSubject(e.Body, subject)
		if e.SideEffect {
			// :then% — subject stays, but we still iterate
		} else {
			subject = result
		}
		if isError(subject) {
			break
		}
	}
	return subject
}

// evalCatchNamed evaluates a :catch [$alias] body step.
func (ev *Evaluator) evalCatchNamed(e *ast.CatchNamedExpression, subject Value) Value {
	if !isError(subject) {
		return subject
	}
	if e.Alias != "" {
		// Bind the error value as a tag so the body can reference it via $alias.
		ev.tags[e.Alias] = subject
	}
	return ev.evalExprWithSubject(e.Body, subject)
}

// evalExprWithSubject evaluates an expression that may reference $ (the subject).
// For call expressions and identifiers it threads subject through as usual.
func (ev *Evaluator) evalExprWithSubject(expr ast.Expression, subject Value) Value {
	switch e := expr.(type) {
	case *ast.CallExpression:
		return ev.evalCall(e, subject)
	case *ast.Identifier:
		if e.Name == "" {
			return subject
		}
		if v, ok := ev.env.get(e.Name); ok {
			return ev.applyValue(v, subject, nil)
		}
		return ev.evalExpr(expr)
	default:
		return ev.evalExpr(expr)
	}
}

// ── Expression evaluation ─────────────────────────────────────────────────────

func (ev *Evaluator) evalExpr(expr ast.Expression) Value {
	switch e := expr.(type) {
	case *ast.IntLiteral:
		return &IntValue{e.Value}
	case *ast.FloatLiteral:
		return &FloatValue{e.Value}
	case *ast.StringLiteral:
		return &StringValue{e.Value}
	case *ast.BoolLiteral:
		return &BoolValue{e.Value}
	case *ast.ArrayLiteral:
		return ev.evalArray(e)
	case *ast.UnaryExpression:
		return ev.evalUnary(e)
	case *ast.BinaryExpression:
		return ev.evalBinary(e)
	case *ast.Identifier:
		return ev.evalIdent(e)
	case *ast.SubjectRef:
		return ev.evalSubjectRef(e)
	case *ast.GroupedExpr:
		return ev.evalExpr(e.Expr)
	case *ast.Pipeline:
		return ev.evalPipeline(e, &NilValue{})
	case *ast.HeadlessPipeline:
		return &FlowValue{Steps: e.Steps, Env: ev.env}
	case *ast.LambdaExpression:
		return &LambdaValue{Param: e.Param, Body: e.Body, Env: ev.env}
	case *ast.CallExpression:
		return ev.evalCall(e, &NilValue{})
	case *ast.MatchExpression:
		return ev.evalMatch(e, &NilValue{})
	default:
		return &NilValue{}
	}
}

func (ev *Evaluator) evalArray(e *ast.ArrayLiteral) Value {
	elems := make([]Value, len(e.Elements))
	for i, el := range e.Elements {
		elems[i] = ev.evalExpr(el)
	}
	return &ArrayValue{elems}
}

func (ev *Evaluator) evalUnary(e *ast.UnaryExpression) Value {
	operand := ev.evalExpr(e.Operand)
	switch e.Op.Kind {
	case token.MINUS:
		switch v := operand.(type) {
		case *IntValue:
			return &IntValue{-v.V}
		case *FloatValue:
			return &FloatValue{-v.V}
		}
	case token.BANG:
		if b, ok := operand.(*BoolValue); ok {
			return &BoolValue{!b.V}
		}
	}
	return &ErrorValue{fmt.Sprintf("unary %s: invalid operand %s", e.Op.Lexeme, operand.Type())}
}

func (ev *Evaluator) evalBinary(e *ast.BinaryExpression) Value {
	left := ev.evalExpr(e.Left)
	right := ev.evalExpr(e.Right)
	return ev.applyOperator(e.Op, left, right)
}

func (ev *Evaluator) applyOperator(op token.Token, left, right Value) Value {
	switch op.Kind {
	case token.PLUS:
		return arith(op, left, right, func(a, b int64) Value { return &IntValue{a + b} },
			func(a, b float64) Value { return &FloatValue{a + b} },
			func(a, b string) Value { return &StringValue{a + b} })
	case token.MINUS:
		return arith(op, left, right, func(a, b int64) Value { return &IntValue{a - b} },
			func(a, b float64) Value { return &FloatValue{a - b} }, nil)
	case token.STAR:
		return arith(op, left, right, func(a, b int64) Value { return &IntValue{a * b} },
			func(a, b float64) Value { return &FloatValue{a * b} }, nil)
	case token.SLASH:
		return arith(op, left, right, func(a, b int64) Value {
			if b == 0 {
				return &ErrorValue{"division by zero"}
			}
			return &IntValue{a / b}
		}, func(a, b float64) Value { return &FloatValue{a / b} }, nil)
	case token.PERCENT:
		return arith(op, left, right, func(a, b int64) Value {
			if b == 0 {
				return &ErrorValue{"modulo by zero"}
			}
			return &IntValue{a % b}
		}, nil, nil)

	case token.EQEQ:
		return &BoolValue{valueEqual(left, right)}
	case token.NEQ:
		return &BoolValue{!valueEqual(left, right)}
	case token.LT:
		return &BoolValue{compareValues(left, right) < 0}
	case token.GT:
		return &BoolValue{compareValues(left, right) > 0}
	case token.LTEQ:
		return &BoolValue{compareValues(left, right) <= 0}
	case token.GTEQ:
		return &BoolValue{compareValues(left, right) >= 0}

	case token.ANDAND:
		lb, lok := left.(*BoolValue)
		rb, rok := right.(*BoolValue)
		if !lok || !rok {
			return &ErrorValue{"&& requires Bool operands"}
		}
		return &BoolValue{lb.V && rb.V}
	case token.OROR:
		lb, lok := left.(*BoolValue)
		rb, rok := right.(*BoolValue)
		if !lok || !rok {
			return &ErrorValue{"|| requires Bool operands"}
		}
		return &BoolValue{lb.V || rb.V}
	}
	return &ErrorValue{fmt.Sprintf("unknown operator %s", op.Lexeme)}
}

func (ev *Evaluator) evalIdent(e *ast.Identifier) Value {
	if v, ok := ev.env.get(e.Name); ok {
		return v
	}
	return &ErrorValue{fmt.Sprintf("undefined variable %q", e.Name)}
}

func (ev *Evaluator) evalSubjectRef(e *ast.SubjectRef) Value {
	if e.Alias == "" {
		return &NilValue{} // bare $ — should be resolved by caller
	}
	if v, ok := ev.tags[e.Alias]; ok {
		return v
	}
	return &ErrorValue{fmt.Sprintf("tag %q not found", e.Alias)}
}

// ── Function calls ────────────────────────────────────────────────────────────

// evalCall evaluates a call expression, threading subject as the implicit
// first argument (unless $ is explicitly placed).
func (ev *Evaluator) evalCall(call *ast.CallExpression, subject Value) Value {
	name := callName(call)

	// Resolve actual args; subject is implicit first arg unless $ is used.
	args := ev.resolveCallArgs(call, subject)

	// Built-in?
	if fn, ok := builtins[name]; ok {
		implicitSubject, rest := splitSubject(args)
		return fn(implicitSubject, rest)
	}

	// User-defined function?
	if v, ok := ev.env.get(name); ok {
		return ev.applyValue(v, subject, args)
	}

	return &ErrorValue{fmt.Sprintf("undefined function %q", name)}
}

// applyValue applies a callable value to a subject and an explicit arg list.
func (ev *Evaluator) applyValue(v Value, subject Value, args []Value) Value {
	switch fn := v.(type) {
	case *FnValue:
		return ev.applyFn(fn, subject, args)
	case *FlowValue:
		return ev.applyFlow(fn, subject)
	case *builtinWrapper:
		implicitSubject, rest := splitSubject(args)
		if args == nil {
			implicitSubject = subject
		}
		return fn.fn(implicitSubject, rest)
	}
	return &ErrorValue{fmt.Sprintf("not callable: %s", v.Type())}
}

// builtinWrapper lets us store a builtinFn as a Value.
type builtinWrapper struct{ fn builtinFn }

func (b *builtinWrapper) Type() string   { return "Builtin" }
func (b *builtinWrapper) String() string { return "<builtin>" }

func (ev *Evaluator) applyFn(fn *FnValue, subject Value, args []Value) Value {
	inner := fn.Env.child()

	// Bind parameters: first param gets subject (unless overridden by args).
	params := fn.Decl.Params
	effectiveArgs := args
	if effectiveArgs == nil {
		effectiveArgs = []Value{subject}
	}

	for i, p := range params {
		if i < len(effectiveArgs) {
			inner.set(p.Name, effectiveArgs[i])
		} else {
			inner.set(p.Name, &NilValue{})
		}
	}

	// Evaluate the body in the new scope.
	saved := ev.env
	ev.env = inner
	defer func() { ev.env = saved }()

	switch b := fn.Decl.Body.(type) {
	case *ast.ExprBody:
		return ev.evalPipeline(b.Pipeline, subject)
	case *ast.BlockBody:
		return ev.evalBlock(b)
	}
	return &NilValue{}
}

func (ev *Evaluator) evalBlock(b *ast.BlockBody) Value {
	var last Value = &NilValue{}
	for _, stmt := range b.Stmts {
		v := ev.evalStmt(stmt)
		if v != nil {
			last = v
		}
		if let, ok := stmt.(*ast.LetStatement); ok {
			ev.env.set(let.Name, v)
			last = v
		}
	}
	return last
}

func (ev *Evaluator) applyFlow(flow *FlowValue, subject Value) Value {
	// Run headless pipeline steps with the given subject.
	savedTags := ev.tags
	ev.tags = copyTags(savedTags)
	defer func() { ev.tags = savedTags }()

	savedEnv := ev.env
	ev.env = flow.Env
	defer func() { ev.env = savedEnv }()

	for _, step := range flow.Steps {
		subject = ev.evalStep(step, subject)
	}
	return subject
}

// resolveCallArgs builds the argument list, replacing $ with subject.
func (ev *Evaluator) resolveCallArgs(call *ast.CallExpression, subject Value) []Value {
	hasExplicitSubject := false
	for _, a := range call.Args {
		if a.IsSubject {
			hasExplicitSubject = true
			break
		}
	}
	var args []Value
	if !hasExplicitSubject {
		args = append(args, subject)
	}
	for _, a := range call.Args {
		if a.IsSubject {
			args = append(args, subject)
		} else {
			args = append(args, ev.evalExpr(a.Value))
		}
	}
	return args
}

// ── Match expression ──────────────────────────────────────────────────────────

func (ev *Evaluator) evalMatch(m *ast.MatchExpression, subject Value) Value {
	for _, arm := range m.Arms {
		// _ is wildcard
		if ident, ok := arm.Pattern.(*ast.Identifier); ok && ident.Name == "_" {
			return ev.applySteps(arm.Steps, subject)
		}
		pattern := ev.evalExpr(arm.Pattern)
		if valueEqual(subject, pattern) {
			return ev.applySteps(arm.Steps, subject)
		}
	}
	return &NilValue{}
}

func (ev *Evaluator) applySteps(steps []ast.PipelineStep, subject Value) Value {
	for _, step := range steps {
		subject = ev.evalStep(step, subject)
		if isError(subject) {
			return subject
		}
	}
	return subject
}

// ── Arithmetic helpers ────────────────────────────────────────────────────────

type intOp func(int64, int64) Value
type floatOp func(float64, float64) Value
type stringOp func(string, string) Value

func arith(op token.Token, left, right Value, iop intOp, fop floatOp, sop stringOp) Value {
	switch l := left.(type) {
	case *IntValue:
		if r, ok := right.(*IntValue); ok && iop != nil {
			return iop(l.V, r.V)
		}
		if r, ok := right.(*FloatValue); ok && fop != nil {
			return fop(float64(l.V), r.V)
		}
	case *FloatValue:
		if r, ok := right.(*FloatValue); ok && fop != nil {
			return fop(l.V, r.V)
		}
		if r, ok := right.(*IntValue); ok && fop != nil {
			return fop(l.V, float64(r.V))
		}
	case *StringValue:
		if r, ok := right.(*StringValue); ok && sop != nil {
			return sop(l.V, r.V)
		}
	}
	return &ErrorValue{fmt.Sprintf("operator %s: incompatible types %s and %s",
		op.Lexeme, left.Type(), right.Type())}
}

func valueEqual(a, b Value) bool {
	switch av := a.(type) {
	case *IntValue:
		if bv, ok := b.(*IntValue); ok {
			return av.V == bv.V
		}
	case *FloatValue:
		if bv, ok := b.(*FloatValue); ok {
			return av.V == bv.V
		}
	case *StringValue:
		if bv, ok := b.(*StringValue); ok {
			return av.V == bv.V
		}
	case *BoolValue:
		if bv, ok := b.(*BoolValue); ok {
			return av.V == bv.V
		}
	case *NilValue:
		_, ok := b.(*NilValue)
		return ok
	}
	return false
}

func compareValues(a, b Value) int {
	switch av := a.(type) {
	case *IntValue:
		if bv, ok := b.(*IntValue); ok {
			switch {
			case av.V < bv.V:
				return -1
			case av.V > bv.V:
				return 1
			}
			return 0
		}
	case *FloatValue:
		if bv, ok := b.(*FloatValue); ok {
			switch {
			case av.V < bv.V:
				return -1
			case av.V > bv.V:
				return 1
			}
			return 0
		}
	case *StringValue:
		if bv, ok := b.(*StringValue); ok {
			if av.V < bv.V {
				return -1
			}
			if av.V > bv.V {
				return 1
			}
			return 0
		}
	}
	return 0
}

// ── Utility helpers ───────────────────────────────────────────────────────────

func callName(call *ast.CallExpression) string {
	switch e := call.Callee.(type) {
	case *ast.Identifier:
		return e.Name
	case *ast.ScopedIdent:
		return e.Parts[len(e.Parts)-1]
	}
	return ""
}

// splitSubject takes the first element as the subject and returns the rest.
func splitSubject(args []Value) (Value, []Value) {
	if len(args) == 0 {
		return &NilValue{}, nil
	}
	return args[0], args[1:]
}

func copyTags(src map[string]Value) map[string]Value {
	dst := make(map[string]Value, len(src))
	for k, v := range src {
		dst[k] = v
	}
	return dst
}

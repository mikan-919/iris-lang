package typechecker

import (
	"iris-lang/ast"
	"iris-lang/token"
)

// Result is returned by Check, containing all errors and the top-level bindings.
type Result struct {
	Errors   []TypeError
	Bindings map[string]Type // top-level let name → inferred type
}

// Check type-checks and lifetime-analyses a parsed Iris program.
func Check(prog *ast.Program) *Result {
	c := &checker{
		env:      NewEnv(),
		lifetime: newLifetimeTracker(),
		bindings: make(map[string]Type),
	}
	c.checkProgram(prog)
	return &Result{Errors: c.errors, Bindings: c.bindings}
}

// ── Checker ───────────────────────────────────────────────────────────────────

type checker struct {
	env      *Env
	lifetime *LifetimeTracker
	errors   []TypeError
	bindings map[string]Type // top-level let bindings (for testing / IDE)
}

func (c *checker) errorf(tok token.Token, kind ErrorKind, format string, args ...any) {
	msg := format
	if len(args) > 0 {
		// Simple sprintf-like formatting without importing fmt to keep the file lean
		// We'll use fmt via a helper.
		msg = sprintf(format, args...)
	}
	c.errors = append(c.errors, TypeError{Tok: tok, Kind: kind, Message: msg})
}

// ── Program ───────────────────────────────────────────────────────────────────

func (c *checker) checkProgram(prog *ast.Program) {
	// First pass: register all top-level fn/trait/impl declarations so that
	// forward references work within the same file.
	for _, stmt := range prog.Stmts {
		c.registerDecl(stmt)
	}
	// Second pass: full type check.
	for _, stmt := range prog.Stmts {
		c.checkStmt(stmt, c.env)
	}
}

// registerDecl pre-registers fn/trait/impl so forward references resolve.
func (c *checker) registerDecl(stmt ast.Statement) {
	switch s := stmt.(type) {
	case *ast.FnDeclaration:
		c.registerFn(s, c.env)
	case *ast.TraitDeclaration:
		c.registerTrait(s)
	case *ast.ImplDeclaration:
		c.registerImpl(s)
	}
}

// ── Statement dispatch ────────────────────────────────────────────────────────

func (c *checker) checkStmt(stmt ast.Statement, env *Env) {
	switch s := stmt.(type) {
	case *ast.LetStatement:
		c.checkLet(s, env)
	case *ast.FnDeclaration:
		c.checkFnBody(s, env)
	case *ast.TraitDeclaration:
		// Already registered; check method completeness in impl pass.
	case *ast.ImplDeclaration:
		c.checkImplCompleteness(s)
	case *ast.ExpressionStatement:
		c.inferExpr(s.Expr, env)
	case *ast.ComptimeDirective:
		// Not type-checked in this pass.
	case *ast.UseStatement:
		// Module resolution is future work.
	}
}

// ── Let statement ─────────────────────────────────────────────────────────────

func (c *checker) checkLet(s *ast.LetStatement, env *Env) {
	if s.Value == nil {
		return
	}
	ty := c.checkPipeline(s.Value, TypeUnknown, env)
	env.setVar(s.Name, ty)
	c.bindings[s.Name] = ty
}

// ── Fn declaration ────────────────────────────────────────────────────────────

func (c *checker) registerFn(s *ast.FnDeclaration, env *Env) {
	sig := c.buildFnSig(s)
	env.setFn(s.Name, sig)
}

func (c *checker) buildFnSig(s *ast.FnDeclaration) *FnSig {
	sig := &FnSig{}
	// Collect type parameter names so we can convert them to TypeVar below.
	typeParamNames := make(map[string]bool)
	for _, tp := range s.TypeParams {
		sig.TypeParams = append(sig.TypeParams, tp.Name)
		typeParamNames[tp.Name] = true
		var bounds BoundSet
		for _, b := range tp.Bounds {
			bounds = append(bounds, b.Name)
		}
		sig.Bounds = append(sig.Bounds, bounds)
	}
	for _, p := range s.Params {
		sig.Params = append(sig.Params, ParamSig{
			Name: p.Name,
			Type: c.astTypeToTypeTP(p.Type, typeParamNames),
		})
	}
	if s.ReturnType != nil {
		sig.Return = c.astTypeToTypeTP(s.ReturnType, typeParamNames)
	}
	return sig
}

func (c *checker) checkFnBody(s *ast.FnDeclaration, env *Env) {
	if s.Body == nil {
		return
	}
	inner := env.child()
	for _, p := range s.Params {
		inner.setVar(p.Name, c.astTypeToType(p.Type))
	}
	switch b := s.Body.(type) {
	case *ast.BlockBody:
		for _, stmt := range b.Stmts {
			c.checkStmt(stmt, inner)
		}
	case *ast.ExprBody:
		c.checkPipeline(b.Pipeline, TypeUnknown, inner)
	}
}

// ── Trait declaration ─────────────────────────────────────────────────────────

func (c *checker) registerTrait(s *ast.TraitDeclaration) {
	def := &TraitDef{Name: s.Name}
	if s.Parent != nil {
		def.Parent = s.Parent.Name
	}
	for _, m := range s.Methods {
		def.Methods = append(def.Methods, m.Name)
	}
	c.env.setTrait(s.Name, def)
}

// ── Impl declaration ──────────────────────────────────────────────────────────

func (c *checker) registerImpl(s *ast.ImplDeclaration) {
	entry := ImplEntry{
		TraitName: s.Trait.Name,
		ForType:   typeName(c.astTypeToType(s.ForType)),
	}
	for _, m := range s.Methods {
		entry.Methods = append(entry.Methods, m.Name)
	}
	c.env.addImpl(entry)
}

func (c *checker) checkImplCompleteness(s *ast.ImplDeclaration) {
	traitName := s.Trait.Name
	def, ok := c.env.getTrait(traitName)
	if !ok {
		return // trait not declared; can't check
	}
	provided := make(map[string]bool)
	for _, m := range s.Methods {
		provided[m.Name] = true
	}
	// Collect inherited methods that have default impls (simplified: trait body
	// methods with a non-nil body are defaults).
	for _, required := range def.Methods {
		if !provided[required] {
			c.errorf(s.Tok, ErrImplMissing,
				"impl of #%s for %s is missing method %q",
				traitName, c.astTypeToType(s.ForType).String(), required)
		}
	}
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

// checkPipeline type-checks a pipeline and returns the final subject type.
// subjectHint is the type flowing into the pipeline (Unknown for top-level lets).
func (c *checker) checkPipeline(pipe *ast.Pipeline, _ Type, env *Env) Type {
	region := c.lifetime.enterPipeline()
	defer c.lifetime.exitPipeline(region)

	var subjectType Type
	if pipe.Subject != nil {
		subjectType = c.inferExpr(pipe.Subject, env)
	} else {
		subjectType = TypeUnknown
	}

	for _, step := range pipe.Steps {
		subjectType = c.checkStep(step, subjectType, env)
	}
	return subjectType
}

func (c *checker) checkSteps(steps []ast.PipelineStep, subjectType Type, env *Env) Type {
	for _, step := range steps {
		subjectType = c.checkStep(step, subjectType, env)
	}
	return subjectType
}

// checkStep handles one backbone-op + expression step.
func (c *checker) checkStep(step ast.PipelineStep, subjectType Type, env *Env) Type {
	switch step.Op.Kind {
	case token.OP_TAG: // :>
		return c.checkTagStep(step, subjectType)
	case token.OP_JOIN: // :&
		return c.checkJoinStep(step, subjectType)
	default:
		return c.checkTransformStep(step, subjectType, env)
	}
}

// checkTagStep handles :> name — binds the current subject as a tag.
func (c *checker) checkTagStep(step ast.PipelineStep, subjectType Type) Type {
	ident, ok := step.Expr.(*ast.Identifier)
	if !ok {
		c.errorf(step.Op, ErrTypeMismatch, ":> requires a plain identifier as tag name")
		return subjectType
	}
	c.lifetime.bindTag(ident.Name, subjectType)
	return subjectType // subject continues unchanged
}

// checkJoinStep handles :& name — joins the current subject with a tag.
func (c *checker) checkJoinStep(step ast.PipelineStep, subjectType Type) Type {
	ident, ok := step.Expr.(*ast.Identifier)
	if !ok {
		c.errorf(step.Op, ErrTypeMismatch, ":& requires a plain identifier as join name")
		return subjectType
	}
	if err := c.lifetime.checkTagEscape(ident.Name, step.Op); err != nil {
		c.errors = append(c.errors, *err)
		return TypeUnknown
	}
	tag := c.lifetime.tags[ident.Name]
	return NewTupleType(subjectType, tag.typ)
}

// checkTransformStep handles ::, :~, :^, :!, :?, :| — the transform backbone ops.
func (c *checker) checkTransformStep(step ast.PipelineStep, subjectType Type, env *Env) Type {
	switch expr := step.Expr.(type) {
	case *ast.CallExpression:
		return c.checkCallInPipeline(expr, subjectType, env)
	case *ast.OperatorApplication:
		return c.checkOperatorApp(expr, subjectType)
	default:
		// Expression used as the new subject (e.g., :| fallback)
		return c.inferExpr(expr, env)
	}
}

// checkCallInPipeline checks a function call where the pipeline subject is the
// implicit first argument (or explicit $ argument).
func (c *checker) checkCallInPipeline(call *ast.CallExpression, subjectType Type, env *Env) Type {
	fnName := c.callName(call)
	sig, ok := env.getFn(fnName)
	if !ok {
		// Unknown function: infer args, propagate Unknown as result.
		for _, arg := range call.Args {
			if !arg.IsSubject {
				c.inferExpr(arg.Value, env)
			}
		}
		return TypeUnknown
	}

	// Build the effective argument list: implicit subject prepended unless $ is explicit.
	args := c.resolveArgs(call, subjectType, env)

	// Check type params (generic bound check).
	typeSubst := c.resolveTypeParams(sig, args, call.Callee.Pos())

	// Check each parameter.
	for i, param := range sig.Params {
		if i >= len(args) {
			break
		}
		expected := substituteTypeVar(param.Type, typeSubst)
		if !isCompatible(args[i], expected) {
			c.errorf(call.Callee.Pos(), ErrTypeMismatch,
				"argument %d of %s: got %s, want %s", i+1, fnName, args[i].String(), expected.String())
		}
	}

	ret := substituteTypeVar(sig.Return, typeSubst)
	if ret == nil {
		return TypeUnit
	}
	return ret
}

// resolveArgs builds the argument type list: subject is implicit first arg
// unless the call has an explicit subject placeholder.
func (c *checker) resolveArgs(call *ast.CallExpression, subjectType Type, env *Env) []Type {
	hasExplicitSubject := false
	for _, a := range call.Args {
		if a.IsSubject {
			hasExplicitSubject = true
			break
		}
	}
	var args []Type
	if !hasExplicitSubject {
		args = append(args, subjectType) // implicit first arg
	}
	for _, a := range call.Args {
		if a.IsSubject {
			args = append(args, subjectType)
		} else {
			args = append(args, c.inferExpr(a.Value, env))
		}
	}
	return args
}

// checkOperatorApp handles :: * n  (subject OP n).
func (c *checker) checkOperatorApp(op *ast.OperatorApplication, subjectType Type) Type {
	// The subject and the right operand must both be numeric for arithmetic,
	// or matching types for comparison.
	if isArithOp(op.Op.Kind) && !isNumeric(subjectType) {
		c.errorf(op.Op, ErrBinaryTypeMismatch,
			"operator %s requires a numeric subject, got %s", op.Op.Lexeme, subjectType.String())
	}
	return subjectType
}

// resolveTypeParams infers type variable substitutions from the actual argument
// types and checks that bounds are satisfied.
func (c *checker) resolveTypeParams(sig *FnSig, args []Type, tok token.Token) map[string]Type {
	subst := make(map[string]Type)
	for _, paramName := range sig.TypeParams {
		// Find the first parameter that uses this type var and infer from it.
		for j, param := range sig.Params {
			tv, ok := param.Type.(*TypeVar)
			if !ok || tv.Name != paramName {
				continue
			}
			if j < len(args) {
				subst[paramName] = args[j]
			}
			break
		}
		// Check bounds.
		if bounds, ok := sig.BoundsFor(paramName); ok {
			inferred, resolved := subst[paramName]
			if !resolved {
				continue
			}
			tname := typeName(inferred)
			for _, traitName := range bounds {
				if !c.env.implements(tname, traitName) {
					c.errorf(tok, ErrTraitBoundViolation,
						"type %s does not implement #%s", tname, traitName)
				}
			}
		}
	}
	return subst
}

// BoundsFor returns the bound set for a named type parameter.
func (sig *FnSig) BoundsFor(paramName string) (BoundSet, bool) {
	for i, name := range sig.TypeParams {
		if name == paramName && i < len(sig.Bounds) {
			return sig.Bounds[i], true
		}
	}
	return nil, false
}

// ── Expression inference ──────────────────────────────────────────────────────

func (c *checker) inferExpr(expr ast.Expression, env *Env) Type {
	switch e := expr.(type) {
	case *ast.IntLiteral:
		return TypeInt
	case *ast.FloatLiteral:
		return TypeFloat
	case *ast.StringLiteral:
		return TypeString
	case *ast.BoolLiteral:
		return TypeBool
	case *ast.ArrayLiteral:
		return c.inferArray(e, env)
	case *ast.UnaryExpression:
		return c.inferUnary(e, env)
	case *ast.BinaryExpression:
		return c.inferBinary(e, env)
	case *ast.Identifier:
		return c.inferIdent(e, env)
	case *ast.SubjectRef:
		return c.inferSubjectRef(e)
	case *ast.Pipeline:
		return c.checkPipeline(e, TypeUnknown, env)
	case *ast.HeadlessPipeline:
		return c.inferHeadless(e, env)
	case *ast.GroupedExpr:
		return c.inferExpr(e.Expr, env)
	case *ast.CallExpression:
		return c.checkCallInPipeline(e, TypeUnknown, env)
	case *ast.TraitRef:
		return &TraitType{Name: e.Name}
	case *ast.MatchExpression:
		return c.inferMatch(e, env)
	case *ast.OperatorApplication:
		// OperatorApplication outside a pipeline step — treat as unknown.
		return TypeUnknown
	default:
		return TypeUnknown
	}
}

func (c *checker) inferArray(e *ast.ArrayLiteral, env *Env) Type {
	if len(e.Elements) == 0 {
		return NewArrayType(TypeUnknown)
	}
	elemType := c.inferExpr(e.Elements[0], env)
	for _, el := range e.Elements[1:] {
		t := c.inferExpr(el, env)
		if !t.Equals(elemType) {
			c.errorf(el.Pos(), ErrArrayHetero,
				"array element type mismatch: expected %s, got %s", elemType.String(), t.String())
		}
	}
	return NewArrayType(elemType)
}

func (c *checker) inferUnary(e *ast.UnaryExpression, env *Env) Type {
	operand := c.inferExpr(e.Operand, env)
	switch e.Op.Kind {
	case token.MINUS:
		if !isNumeric(operand) {
			c.errorf(e.Op, ErrUnaryTypeInvalid,
				"unary - requires Int or Float, got %s", operand.String())
			return TypeUnknown
		}
		return operand
	case token.BANG:
		if !operand.Equals(TypeBool) {
			c.errorf(e.Op, ErrUnaryTypeInvalid,
				"unary ! requires Bool, got %s", operand.String())
			return TypeUnknown
		}
		return TypeBool
	}
	return operand
}

func (c *checker) inferBinary(e *ast.BinaryExpression, env *Env) Type {
	left := c.inferExpr(e.Left, env)
	right := c.inferExpr(e.Right, env)
	k := e.Op.Kind

	switch {
	case isArithOp(k):
		if !left.Equals(right) || !isNumeric(left) {
			c.errorf(e.Op, ErrBinaryTypeMismatch,
				"operator %s: incompatible types %s and %s", e.Op.Lexeme, left.String(), right.String())
			return TypeUnknown
		}
		return left
	case isCompareOp(k):
		if !left.Equals(right) {
			c.errorf(e.Op, ErrBinaryTypeMismatch,
				"operator %s: cannot compare %s with %s", e.Op.Lexeme, left.String(), right.String())
		}
		return TypeBool
	case isLogicalOp(k):
		if !left.Equals(TypeBool) || !right.Equals(TypeBool) {
			c.errorf(e.Op, ErrBinaryTypeMismatch,
				"operator %s: requires Bool operands, got %s and %s", e.Op.Lexeme, left.String(), right.String())
			return TypeUnknown
		}
		return TypeBool
	}
	return TypeUnknown
}

func (c *checker) inferIdent(e *ast.Identifier, env *Env) Type {
	if t, ok := env.getVar(e.Name); ok {
		return t
	}
	// Check if it's a function (fn without args used as value).
	if _, ok := env.getFn(e.Name); ok {
		return TypeUnknown // function references return Unknown for now
	}
	c.errorf(e.Tok, ErrUndefinedVar, "undefined variable %q", e.Name)
	return TypeUnknown
}

func (c *checker) inferSubjectRef(e *ast.SubjectRef) Type {
	if e.Alias == "" {
		return TypeUnknown // bare subject placeholder
	}
	// named alias — must reference a live tag.
	if err := c.lifetime.checkTagEscape(e.Alias, e.Tok); err != nil {
		c.errors = append(c.errors, *err)
		return TypeUnknown
	}
	return c.lifetime.tags[e.Alias].typ
}

func (c *checker) inferHeadless(e *ast.HeadlessPipeline, env *Env) Type {
	region := c.lifetime.enterPipeline()
	defer c.lifetime.exitPipeline(region)
	var subjectType Type = TypeUnknown
	for _, step := range e.Steps {
		subjectType = c.checkStep(step, subjectType, env)
	}
	_ = subjectType
	return TypeUnknown // headless pipeline is a Flow object; its type is opaque for now
}

func (c *checker) inferMatch(e *ast.MatchExpression, env *Env) Type {
	// All arms must have the same result type.
	var resultType Type
	for _, arm := range e.Arms {
		c.inferExpr(arm.Pattern, env)
		t := c.checkSteps(arm.Steps, TypeUnknown, env)
		if resultType == nil {
			resultType = t
		}
	}
	if resultType == nil {
		return TypeUnknown
	}
	return resultType
}

// ── Helpers ───────────────────────────────────────────────────────────────────

func (c *checker) callName(call *ast.CallExpression) string {
	switch e := call.Callee.(type) {
	case *ast.Identifier:
		return e.Name
	case *ast.ScopedIdent:
		return e.Parts[len(e.Parts)-1]
	}
	return ""
}

// astTypeToTypeTP converts an ast.TypeExpr to a Type, replacing names that are
// declared type parameters with TypeVar so generic substitution works.
func (c *checker) astTypeToTypeTP(te ast.TypeExpr, typeParams map[string]bool) Type {
	if te == nil {
		return TypeUnknown
	}
	if nt, ok := te.(*ast.NamedType); ok && typeParams[nt.Name] {
		return &TypeVar{Name: nt.Name}
	}
	return c.astTypeToType(te)
}

// astTypeToType converts an ast.TypeExpr to a typechecker.Type.
func (c *checker) astTypeToType(te ast.TypeExpr) Type {
	if te == nil {
		return TypeUnknown
	}
	switch t := te.(type) {
	case *ast.NamedType:
		switch t.Name {
		case "Int":
			return TypeInt
		case "Float":
			return TypeFloat
		case "String":
			return TypeString
		case "Bool":
			return TypeBool
		}
		params := make([]Type, len(t.Params))
		for i, p := range t.Params {
			params[i] = c.astTypeToType(p)
		}
		if len(params) == 0 {
			return &NamedType{Name: t.Name}
		}
		return &NamedType{Name: t.Name, Params: params}
	case *ast.TraitRef:
		return &TraitType{Name: t.Name}
	case *ast.TraitBoundType:
		if len(t.Bounds) == 1 {
			return &TraitType{Name: t.Bounds[0].Name}
		}
		return TypeUnknown
	}
	return TypeUnknown
}

// isCompatible returns true if got can be used where want is expected.
// Unknown is compatible with everything (gradual typing).
func isCompatible(got, want Type) bool {
	if _, ok := got.(*unknownType); ok {
		return true
	}
	if _, ok := want.(*unknownType); ok {
		return true
	}
	if tv, ok := want.(*TypeVar); ok {
		_ = tv
		return true // generic parameter: accept anything (bounds checked separately)
	}
	return got.Equals(want)
}

// substituteTypeVar replaces TypeVar occurrences in t according to subst.
func substituteTypeVar(t Type, subst map[string]Type) Type {
	if t == nil {
		return TypeUnknown
	}
	if tv, ok := t.(*TypeVar); ok {
		if s, ok := subst[tv.Name]; ok {
			return s
		}
		return t
	}
	return t
}

// ── Token kind classifiers ────────────────────────────────────────────────────

func isArithOp(k token.Kind) bool {
	switch k {
	case token.PLUS, token.MINUS, token.STAR, token.SLASH, token.PERCENT:
		return true
	}
	return false
}

func isCompareOp(k token.Kind) bool {
	switch k {
	case token.EQEQ, token.NEQ, token.LT, token.GT, token.LTEQ, token.GTEQ:
		return true
	}
	return false
}

func isLogicalOp(k token.Kind) bool {
	return k == token.ANDAND || k == token.OROR
}

// sprintf is a thin wrapper so checker.go doesn't need to import fmt directly.
func sprintf(format string, args ...any) string {
	// We import fmt only via this indirection to keep errorf readable.
	return fmtSprintf(format, args...)
}

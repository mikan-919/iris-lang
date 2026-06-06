package parser_test

import (
	"iris-lang/ast"
	"iris-lang/lexer"
	"iris-lang/parser"
	"iris-lang/token"
	"strings"
	"testing"
)

// parse runs the parser and returns the program. Fails the test on any errors.
func parse(t *testing.T, src string) *ast.Program {
	t.Helper()
	prog := parser.New(lexer.New(src)).ParseProgram()
	if len(prog.Errors) > 0 {
		var msgs []string
		for _, e := range prog.Errors {
			msgs = append(msgs, e.Error())
		}
		t.Fatalf("parse errors:\n  %s\nsrc:\n%s", strings.Join(msgs, "\n  "), src)
	}
	return prog
}

// parseNoFail runs the parser even if there are errors (for error-case tests).
func parseNoFail(src string) *ast.Program {
	return parser.New(lexer.New(src)).ParseProgram()
}

func stmtCount(t *testing.T, prog *ast.Program, want int) {
	t.Helper()
	if len(prog.Stmts) != want {
		t.Fatalf("statement count: got %d, want %d", len(prog.Stmts), want)
	}
}

// ── Comptime directives ───────────────────────────────────────────────────────

func TestComptimeDirective(t *testing.T) {
	prog := parse(t, "!require(#Readable)")
	stmtCount(t, prog, 1)
	dir, ok := prog.Stmts[0].(*ast.ComptimeDirective)
	if !ok {
		t.Fatalf("expected ComptimeDirective, got %T", prog.Stmts[0])
	}
	if dir.Name != "require" {
		t.Errorf("name: got %q, want %q", dir.Name, "require")
	}
	if len(dir.Args) != 1 {
		t.Fatalf("args: got %d, want 1", len(dir.Args))
	}
	ref, ok := dir.Args[0].(*ast.TraitRef)
	if !ok {
		t.Fatalf("arg[0]: expected TraitRef, got %T", dir.Args[0])
	}
	if ref.Name != "Readable" {
		t.Errorf("trait name: got %q, want %q", ref.Name, "Readable")
	}
}

func TestMultipleComptimeDirectives(t *testing.T) {
	src := "!require(#Readable)\n!require(#Writable)"
	prog := parse(t, src)
	stmtCount(t, prog, 2)
	for i, stmt := range prog.Stmts {
		if _, ok := stmt.(*ast.ComptimeDirective); !ok {
			t.Errorf("stmt[%d]: expected ComptimeDirective, got %T", i, stmt)
		}
	}
}

// ── Use statement ─────────────────────────────────────────────────────────────

func TestUseStatement(t *testing.T) {
	prog := parse(t, "use std")
	stmtCount(t, prog, 1)
	u, ok := prog.Stmts[0].(*ast.UseStatement)
	if !ok {
		t.Fatalf("expected UseStatement, got %T", prog.Stmts[0])
	}
	if u.Path != "std" {
		t.Errorf("path: got %q, want %q", u.Path, "std")
	}
}

// ── Let statement ─────────────────────────────────────────────────────────────

func TestLetWithLiteral(t *testing.T) {
	prog := parse(t, `let x = 42`)
	stmtCount(t, prog, 1)
	let, ok := prog.Stmts[0].(*ast.LetStatement)
	if !ok {
		t.Fatalf("expected LetStatement, got %T", prog.Stmts[0])
	}
	if let.Name != "x" {
		t.Errorf("name: got %q, want %q", let.Name, "x")
	}
	if let.Value == nil {
		t.Fatal("value is nil")
	}
	// Subject should be IntLiteral(42)
	intLit, ok := let.Value.Subject.(*ast.IntLiteral)
	if !ok {
		t.Fatalf("subject: expected IntLiteral, got %T", let.Value.Subject)
	}
	if intLit.Value != 42 {
		t.Errorf("int value: got %d, want 42", intLit.Value)
	}
}

func TestLetWithPipeline(t *testing.T) {
	src := `let result = input :: trim() :: lower()`
	prog := parse(t, src)
	stmtCount(t, prog, 1)

	let := prog.Stmts[0].(*ast.LetStatement)
	if let.Value.Subject.(*ast.Identifier).Name != "input" {
		t.Errorf("subject: got %v, want %q", let.Value.Subject, "input")
	}
	if len(let.Value.Steps) != 2 {
		t.Fatalf("steps: got %d, want 2", len(let.Value.Steps))
	}
}

func TestLetMultiline(t *testing.T) {
	src := "let result\n= initialValue\n:: processA()\n:tag snapshot\n:: processB()"
	prog := parse(t, src)
	stmtCount(t, prog, 1)

	let := prog.Stmts[0].(*ast.LetStatement)
	if len(let.Value.Steps) != 3 {
		t.Errorf("steps: got %d, want 3", len(let.Value.Steps))
	}
}

// ── Expression statement / pipeline ──────────────────────────────────────────

func TestSimplePipeline(t *testing.T) {
	src := "text :: trim() :: split(\",\")"
	prog := parse(t, src)
	stmtCount(t, prog, 1)

	pipe, ok := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	if !ok {
		t.Fatalf("expected Pipeline, got %T", prog.Stmts[0].(*ast.ExpressionStatement).Expr)
	}
	if pipe.Subject.(*ast.Identifier).Name != "text" {
		t.Errorf("subject: want %q", "text")
	}
	if len(pipe.Steps) != 2 {
		t.Fatalf("steps: got %d, want 2", len(pipe.Steps))
	}
}

func TestAllBackboneOps(t *testing.T) {
	src := `value :: next() :await :try :or fallback :tag snap :join snap`
	prog := parse(t, src)
	stmtCount(t, prog, 1)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	if len(pipe.Steps) != 6 {
		t.Errorf("steps: got %d, want 6", len(pipe.Steps))
	}
}

func TestSubjectPlaceholder(t *testing.T) {
	// text :: includes(base, $) — $ is the subject placeholder
	src := `text :: includes(base, $)`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	call := pipe.Steps[0].Expr.(*ast.CallExpression)
	if len(call.Args) != 2 {
		t.Fatalf("args: got %d, want 2", len(call.Args))
	}
	if !call.Args[1].IsSubject {
		t.Error("arg[1] should be subject placeholder")
	}
}

func TestSubjectAlias(t *testing.T) {
	// $name captures subject alias
	src := `text :: foo($x)`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	call := pipe.Steps[0].Expr.(*ast.CallExpression)
	ref, ok := call.Args[0].Value.(*ast.SubjectRef)
	if !ok {
		t.Fatalf("expected SubjectRef, got %T", call.Args[0].Value)
	}
	if ref.Alias != "x" {
		t.Errorf("alias: got %q, want %q", ref.Alias, "x")
	}
}

func TestOperatorApplication(t *testing.T) {
	// input :: * factor — multiply the subject by factor
	src := `input :: * factor`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	opApp, ok := pipe.Steps[0].Expr.(*ast.OperatorApplication)
	if !ok {
		t.Fatalf("expected OperatorApplication, got %T", pipe.Steps[0].Expr)
	}
	if opApp.Op.Lexeme != "*" {
		t.Errorf("op: got %q, want %q", opApp.Op.Lexeme, "*")
	}
}

func TestHeadlessPipeline(t *testing.T) {
	src := "(:: trim() :: lower())"
	prog := parse(t, src)
	stmtCount(t, prog, 1)
	_, ok := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.HeadlessPipeline)
	if !ok {
		t.Fatalf("expected HeadlessPipeline, got %T", prog.Stmts[0].(*ast.ExpressionStatement).Expr)
	}
}

func TestTagStep(t *testing.T) {
	src := "value :tag snapshot"
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	if len(pipe.Steps) != 1 {
		t.Fatalf("steps: got %d, want 1", len(pipe.Steps))
	}
	ident, ok := pipe.Steps[0].Expr.(*ast.Identifier)
	if !ok {
		t.Fatalf("expected Identifier, got %T", pipe.Steps[0].Expr)
	}
	if ident.Name != "snapshot" {
		t.Errorf("tag name: got %q, want %q", ident.Name, "snapshot")
	}
}

// ── Function declarations ─────────────────────────────────────────────────────

func TestFnDeclaration(t *testing.T) {
	src := `fn double(n: Int) -> Int { }`
	prog := parse(t, src)
	stmtCount(t, prog, 1)
	fn, ok := prog.Stmts[0].(*ast.FnDeclaration)
	if !ok {
		t.Fatalf("expected FnDeclaration, got %T", prog.Stmts[0])
	}
	if fn.Name != "double" {
		t.Errorf("name: got %q, want %q", fn.Name, "double")
	}
	if fn.Exported {
		t.Error("should not be exported")
	}
	if len(fn.Params) != 1 {
		t.Fatalf("params: got %d, want 1", len(fn.Params))
	}
	if fn.Params[0].Name != "n" {
		t.Errorf("param name: got %q, want %q", fn.Params[0].Name, "n")
	}
	if fn.ReturnType.(*ast.NamedType).Name != "Int" {
		t.Errorf("return type: got %v, want Int", fn.ReturnType)
	}
}

func TestFnWithExprBody(t *testing.T) {
	src := `fn double(n: Int) -> Int = n :: * 2`
	prog := parse(t, src)
	fn := prog.Stmts[0].(*ast.FnDeclaration)
	if _, ok := fn.Body.(*ast.ExprBody); !ok {
		t.Fatalf("body: expected ExprBody, got %T", fn.Body)
	}
}

func TestExportedFn(t *testing.T) {
	src := `export fn calculate(input: Int) -> Int { }`
	prog := parse(t, src)
	fn := prog.Stmts[0].(*ast.FnDeclaration)
	if !fn.Exported {
		t.Error("expected exported fn")
	}
	if fn.Name != "calculate" {
		t.Errorf("name: got %q, want %q", fn.Name, "calculate")
	}
}

func TestGenericFn(t *testing.T) {
	src := `fn trim<T: #Text>(text: T) -> T { }`
	prog := parse(t, src)
	fn := prog.Stmts[0].(*ast.FnDeclaration)
	if len(fn.TypeParams) != 1 {
		t.Fatalf("type params: got %d, want 1", len(fn.TypeParams))
	}
	if fn.TypeParams[0].Name != "T" {
		t.Errorf("type param name: got %q, want %q", fn.TypeParams[0].Name, "T")
	}
	if len(fn.TypeParams[0].Bounds) != 1 || fn.TypeParams[0].Bounds[0].Name != "Text" {
		t.Errorf("bounds: got %v, want [#Text]", fn.TypeParams[0].Bounds)
	}
}

func TestGenericFnMultipleBounds(t *testing.T) {
	src := `fn save<T: #Text && #Serializable>(v: T) { }`
	prog := parse(t, src)
	fn := prog.Stmts[0].(*ast.FnDeclaration)
	if len(fn.TypeParams[0].Bounds) != 2 {
		t.Fatalf("bounds: got %d, want 2", len(fn.TypeParams[0].Bounds))
	}
}

// ── Trait declarations ────────────────────────────────────────────────────────

func TestTraitDeclaration(t *testing.T) {
	src := "trait #Text {\n  fn trim() -> Self { }\n  fn len() -> Int { }\n}"
	prog := parse(t, src)
	stmtCount(t, prog, 1)
	trait, ok := prog.Stmts[0].(*ast.TraitDeclaration)
	if !ok {
		t.Fatalf("expected TraitDeclaration, got %T", prog.Stmts[0])
	}
	if trait.Name != "Text" {
		t.Errorf("name: got %q, want %q", trait.Name, "Text")
	}
	if len(trait.Methods) != 2 {
		t.Errorf("methods: got %d, want 2", len(trait.Methods))
	}
}

func TestTraitInheritance(t *testing.T) {
	src := "trait #String : #Text { }"
	prog := parse(t, src)
	trait := prog.Stmts[0].(*ast.TraitDeclaration)
	if trait.Parent == nil {
		t.Fatal("parent is nil")
	}
	if trait.Parent.Name != "Text" {
		t.Errorf("parent: got %q, want %q", trait.Parent.Name, "Text")
	}
}

// ── Impl declarations ─────────────────────────────────────────────────────────

func TestImplDeclaration(t *testing.T) {
	src := "impl #String for String {\n  fn trim() -> String { }\n}"
	prog := parse(t, src)
	stmtCount(t, prog, 1)
	impl, ok := prog.Stmts[0].(*ast.ImplDeclaration)
	if !ok {
		t.Fatalf("expected ImplDeclaration, got %T", prog.Stmts[0])
	}
	if impl.Trait.Name != "String" {
		t.Errorf("trait: got %q, want %q", impl.Trait.Name, "String")
	}
	if impl.ForType.(*ast.NamedType).Name != "String" {
		t.Errorf("for type: got %v, want String", impl.ForType)
	}
	if len(impl.Methods) != 1 {
		t.Errorf("methods: got %d, want 1", len(impl.Methods))
	}
}

// ── Match expression ──────────────────────────────────────────────────────────

func TestMatchExpression(t *testing.T) {
	src := `result :: match (
  "ok"  :: upper()
  "err" :: lower()
)`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	m, ok := pipe.Steps[0].Expr.(*ast.MatchExpression)
	if !ok {
		t.Fatalf("expected MatchExpression, got %T", pipe.Steps[0].Expr)
	}
	if len(m.Arms) != 2 {
		t.Errorf("arms: got %d, want 2", len(m.Arms))
	}
}

// ── Binary expressions ────────────────────────────────────────────────────────

func TestBinaryExpression(t *testing.T) {
	src := "let z = a + b"
	prog := parse(t, src)
	let := prog.Stmts[0].(*ast.LetStatement)
	bin, ok := let.Value.Subject.(*ast.BinaryExpression)
	if !ok {
		t.Fatalf("expected BinaryExpression, got %T", let.Value.Subject)
	}
	if bin.Op.Lexeme != "+" {
		t.Errorf("op: got %q, want %q", bin.Op.Lexeme, "+")
	}
}

func TestBinaryPrecedence(t *testing.T) {
	// a + b * c  should parse as  a + (b * c)
	src := "let z = a + b * c"
	prog := parse(t, src)
	let := prog.Stmts[0].(*ast.LetStatement)
	add, ok := let.Value.Subject.(*ast.BinaryExpression)
	if !ok {
		t.Fatalf("expected BinaryExpression, got %T", let.Value.Subject)
	}
	if add.Op.Lexeme != "+" {
		t.Errorf("outer op: got %q, want +", add.Op.Lexeme)
	}
	_, isMul := add.Right.(*ast.BinaryExpression)
	if !isMul {
		t.Error("right side of + should be a BinaryExpression (b * c)")
	}
}

// ── Boolean literals ─────────────────────────────────────────────────────────

func TestBoolLiteralTrue(t *testing.T) {
	prog := parse(t, "let x = true")
	let := prog.Stmts[0].(*ast.LetStatement)
	b, ok := let.Value.Subject.(*ast.BoolLiteral)
	if !ok {
		t.Fatalf("expected BoolLiteral, got %T", let.Value.Subject)
	}
	if !b.Value {
		t.Error("expected true")
	}
}

func TestBoolLiteralFalse(t *testing.T) {
	prog := parse(t, "let x = false")
	let := prog.Stmts[0].(*ast.LetStatement)
	b, ok := let.Value.Subject.(*ast.BoolLiteral)
	if !ok {
		t.Fatalf("expected BoolLiteral, got %T", let.Value.Subject)
	}
	if b.Value {
		t.Error("expected false")
	}
}

func TestBoolLiteralString(t *testing.T) {
	prog := parse(t, "let x = true")
	let := prog.Stmts[0].(*ast.LetStatement)
	if got := let.Value.Subject.String(); got != "true" {
		t.Errorf("String(): got %q, want %q", got, "true")
	}
}

// ── Array literals ────────────────────────────────────────────────────────────

func TestArrayLiteralEmpty(t *testing.T) {
	prog := parse(t, "let x = []")
	let := prog.Stmts[0].(*ast.LetStatement)
	arr, ok := let.Value.Subject.(*ast.ArrayLiteral)
	if !ok {
		t.Fatalf("expected ArrayLiteral, got %T", let.Value.Subject)
	}
	if len(arr.Elements) != 0 {
		t.Errorf("elements: got %d, want 0", len(arr.Elements))
	}
}

func TestArrayLiteralInts(t *testing.T) {
	prog := parse(t, "let x = [1, 2, 3]")
	let := prog.Stmts[0].(*ast.LetStatement)
	arr, ok := let.Value.Subject.(*ast.ArrayLiteral)
	if !ok {
		t.Fatalf("expected ArrayLiteral, got %T", let.Value.Subject)
	}
	if len(arr.Elements) != 3 {
		t.Fatalf("elements: got %d, want 3", len(arr.Elements))
	}
	for i, el := range arr.Elements {
		lit, ok := el.(*ast.IntLiteral)
		if !ok {
			t.Errorf("element[%d]: expected IntLiteral, got %T", i, el)
			continue
		}
		if lit.Value != int64(i+1) {
			t.Errorf("element[%d]: got %d, want %d", i, lit.Value, i+1)
		}
	}
}

func TestArrayLiteralExpressions(t *testing.T) {
	prog := parse(t, `let x = ["a", "b"]`)
	let := prog.Stmts[0].(*ast.LetStatement)
	arr, ok := let.Value.Subject.(*ast.ArrayLiteral)
	if !ok {
		t.Fatalf("expected ArrayLiteral, got %T", let.Value.Subject)
	}
	if len(arr.Elements) != 2 {
		t.Errorf("elements: got %d, want 2", len(arr.Elements))
	}
}

func TestArrayLiteralString(t *testing.T) {
	prog := parse(t, "let x = [1, 2]")
	let := prog.Stmts[0].(*ast.LetStatement)
	if got := let.Value.Subject.String(); got != "[1, 2]" {
		t.Errorf("String(): got %q, want %q", got, "[1, 2]")
	}
}

// ── Unary expressions ─────────────────────────────────────────────────────────

func TestUnaryMinus(t *testing.T) {
	prog := parse(t, "let x = -42")
	let := prog.Stmts[0].(*ast.LetStatement)
	u, ok := let.Value.Subject.(*ast.UnaryExpression)
	if !ok {
		t.Fatalf("expected UnaryExpression, got %T", let.Value.Subject)
	}
	if u.Op.Lexeme != "-" {
		t.Errorf("op: got %q, want -", u.Op.Lexeme)
	}
	if _, ok := u.Operand.(*ast.IntLiteral); !ok {
		t.Errorf("operand: expected IntLiteral, got %T", u.Operand)
	}
}

func TestUnaryNot(t *testing.T) {
	// Inside an expression, ! is unary NOT (not a comptime directive)
	prog := parse(t, "let x = !flag")
	let := prog.Stmts[0].(*ast.LetStatement)
	u, ok := let.Value.Subject.(*ast.UnaryExpression)
	if !ok {
		t.Fatalf("expected UnaryExpression, got %T", let.Value.Subject)
	}
	if u.Op.Lexeme != "!" {
		t.Errorf("op: got %q, want !", u.Op.Lexeme)
	}
}

func TestUnaryString(t *testing.T) {
	prog := parse(t, "let x = -1")
	let := prog.Stmts[0].(*ast.LetStatement)
	if got := let.Value.Subject.String(); got != "-1" {
		t.Errorf("String(): got %q, want %q", got, "-1")
	}
}

// ── :if / :then / :else ───────────────────────────────────────────────────────

func TestIfThenElse(t *testing.T) {
	src := `value :if condition :then yes :else no`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	ifExpr, ok := pipe.Steps[0].Expr.(*ast.IfExpression)
	if !ok {
		t.Fatalf("expected IfExpression, got %T", pipe.Steps[0].Expr)
	}
	if ifExpr.Condition.(*ast.Identifier).Name != "condition" {
		t.Errorf("condition: got %v", ifExpr.Condition)
	}
	if ifExpr.Then.(*ast.Identifier).Name != "yes" {
		t.Errorf("then: got %v", ifExpr.Then)
	}
	if ifExpr.Else.(*ast.Identifier).Name != "no" {
		t.Errorf("else: got %v", ifExpr.Else)
	}
}

func TestIfThenWithoutElse(t *testing.T) {
	src := `value :if ok :then doIt`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	ifExpr := pipe.Steps[0].Expr.(*ast.IfExpression)
	if ifExpr.Else != nil {
		t.Error("expected no :else")
	}
}

func TestIfThenSideEffect(t *testing.T) {
	src := `value :if ok :then% log`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	ifExpr := pipe.Steps[0].Expr.(*ast.IfExpression)
	if !ifExpr.SideEffect {
		t.Error("expected SideEffect = true for :then%")
	}
}

// ── :while / :then ────────────────────────────────────────────────────────────

func TestWhileThen(t *testing.T) {
	src := `stream :while hasNext :then process`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	whileExpr, ok := pipe.Steps[0].Expr.(*ast.WhileExpression)
	if !ok {
		t.Fatalf("expected WhileExpression, got %T", pipe.Steps[0].Expr)
	}
	if whileExpr.Condition.(*ast.Identifier).Name != "hasNext" {
		t.Errorf("condition: got %v", whileExpr.Condition)
	}
	if whileExpr.Body.(*ast.Identifier).Name != "process" {
		t.Errorf("body: got %v", whileExpr.Body)
	}
}

func TestWhileThenSideEffect(t *testing.T) {
	src := `value :while running :then% log`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	whileExpr := pipe.Steps[0].Expr.(*ast.WhileExpression)
	if !whileExpr.SideEffect {
		t.Error("expected SideEffect = true for :then%")
	}
}

// ── :catch $alias ─────────────────────────────────────────────────────────────

func TestCatchNamed(t *testing.T) {
	src := `value :catch $err recover`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	catchExpr, ok := pipe.Steps[0].Expr.(*ast.CatchNamedExpression)
	if !ok {
		t.Fatalf("expected CatchNamedExpression, got %T", pipe.Steps[0].Expr)
	}
	if catchExpr.Alias != "err" {
		t.Errorf("alias: got %q, want %q", catchExpr.Alias, "err")
	}
}

func TestCatchNamedNoAlias(t *testing.T) {
	src := `value :catch recover`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	catchExpr, ok := pipe.Steps[0].Expr.(*ast.CatchNamedExpression)
	if !ok {
		t.Fatalf("expected CatchNamedExpression, got %T", pipe.Steps[0].Expr)
	}
	if catchExpr.Alias != "" {
		t.Errorf("expected empty alias, got %q", catchExpr.Alias)
	}
}

// ── :await ────────────────────────────────────────────────────────────────────

func TestAwaitKeyword(t *testing.T) {
	src := `promise :await`
	prog := parse(t, src)
	pipe := prog.Stmts[0].(*ast.ExpressionStatement).Expr.(*ast.Pipeline)
	if pipe.Steps[0].Op.Kind != token.OP_AWAIT {
		t.Errorf("op kind: got %v, want OP_AWAIT", pipe.Steps[0].Op.Kind)
	}
}

// ── Error recovery ────────────────────────────────────────────────────────────

func TestMissingInitiateError(t *testing.T) {
	prog := parseNoFail("let x foo")
	if len(prog.Errors) == 0 {
		t.Error("expected parse error for missing =")
	}
}

// ── Sample file ───────────────────────────────────────────────────────────────

func TestSampleFile(t *testing.T) {
	src := `!require(#String)

text
:: trim()
:: split(",")
`
	prog := parse(t, src)
	if len(prog.Stmts) != 2 {
		t.Errorf("statements: got %d, want 2", len(prog.Stmts))
	}
}

// ── String representation sanity ──────────────────────────────────────────────

func TestStringRoundtrip(t *testing.T) {
	cases := []string{
		`!require(#Readable)`,
		`use std`,
		`let x = 42`,
		`fn double(n: Int) -> Int { }`,
		`export fn add(a: Int, b: Int) -> Int { }`,
		`trait #Text { }`,
		`trait #String : #Text { }`,
		`impl #String for String { }`,
	}
	for _, src := range cases {
		t.Run(src, func(t *testing.T) {
			prog := parse(t, src)
			if len(prog.Stmts) == 0 {
				t.Fatal("no statements parsed")
			}
			// String() should not panic
			s := prog.Stmts[0].String()
			if s == "" {
				t.Error("String() returned empty string")
			}
		})
	}
}

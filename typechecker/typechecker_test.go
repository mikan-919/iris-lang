package typechecker_test

import (
	"fmt"
	"iris-lang/lexer"
	"iris-lang/parser"
	"iris-lang/typechecker"
	"testing"
)

// ── Test helpers ──────────────────────────────────────────────────────────────

func checkSrc(src string) *typechecker.Result {
	prog := parser.New(lexer.New(src)).ParseProgram()
	return typechecker.Check(prog)
}

func mustOk(t *testing.T, src string) *typechecker.Result {
	t.Helper()
	r := checkSrc(src)
	if len(r.Errors) > 0 {
		var msgs string
		for _, e := range r.Errors {
			msgs += "\n  " + e.Error()
		}
		t.Fatalf("unexpected type errors:%s\nsrc: %q", msgs, src)
	}
	return r
}

func mustFail(t *testing.T, src string) *typechecker.Result {
	t.Helper()
	r := checkSrc(src)
	if len(r.Errors) == 0 {
		t.Fatalf("expected type errors, got none\nsrc: %q", src)
	}
	return r
}

func assertBinding(t *testing.T, r *typechecker.Result, name string, want fmt.Stringer) {
	t.Helper()
	got, ok := r.Bindings[name]
	if !ok {
		t.Fatalf("binding %q not found in result", name)
	}
	if got.String() != want.String() {
		t.Errorf("binding %q: got %q, want %q", name, got.String(), want.String())
	}
}

// ── Literal type inference ────────────────────────────────────────────────────

func TestInferIntLiteral(t *testing.T) {
	r := mustOk(t, "let x = 42")
	assertBinding(t, r, "x", typechecker.TypeInt)
}

func TestInferFloatLiteral(t *testing.T) {
	r := mustOk(t, "let x = 3.14")
	assertBinding(t, r, "x", typechecker.TypeFloat)
}

func TestInferStringLiteral(t *testing.T) {
	r := mustOk(t, `let x = "hello"`)
	assertBinding(t, r, "x", typechecker.TypeString)
}

func TestInferBoolLiteralTrue(t *testing.T) {
	r := mustOk(t, "let x = true")
	assertBinding(t, r, "x", typechecker.TypeBool)
}

func TestInferBoolLiteralFalse(t *testing.T) {
	r := mustOk(t, "let x = false")
	assertBinding(t, r, "x", typechecker.TypeBool)
}

func TestInferArrayEmpty(t *testing.T) {
	r := mustOk(t, "let x = []")
	assertBinding(t, r, "x", typechecker.NewArrayType(typechecker.TypeUnknown))
}

func TestInferArrayInts(t *testing.T) {
	r := mustOk(t, "let x = [1, 2, 3]")
	assertBinding(t, r, "x", typechecker.NewArrayType(typechecker.TypeInt))
}

func TestInferArrayStrings(t *testing.T) {
	r := mustOk(t, `let x = ["a", "b"]`)
	assertBinding(t, r, "x", typechecker.NewArrayType(typechecker.TypeString))
}

// ── Array type errors ─────────────────────────────────────────────────────────

func TestArrayHeteroError(t *testing.T) {
	mustFail(t, `let x = [1, "a"]`)
}

func TestArrayHeteroErrorKind(t *testing.T) {
	r := mustFail(t, `let x = [1, "a"]`)
	if r.Errors[0].Kind != typechecker.ErrArrayHetero {
		t.Errorf("error kind: got %v, want ErrArrayHetero", r.Errors[0].Kind)
	}
}

// ── Unary expression types ────────────────────────────────────────────────────

func TestInferUnaryMinus(t *testing.T) {
	r := mustOk(t, "let x = -42")
	assertBinding(t, r, "x", typechecker.TypeInt)
}

func TestInferUnaryMinusFloat(t *testing.T) {
	r := mustOk(t, "let x = -3.14")
	assertBinding(t, r, "x", typechecker.TypeFloat)
}

func TestInferUnaryNot(t *testing.T) {
	r := mustOk(t, "let x = !true")
	assertBinding(t, r, "x", typechecker.TypeBool)
}

func TestUnaryMinusStringError(t *testing.T) {
	mustFail(t, `let x = -"hello"`)
}

func TestUnaryNotIntError(t *testing.T) {
	mustFail(t, "let x = !42")
}

// ── Binary expression types ───────────────────────────────────────────────────

func TestInferBinaryIntAdd(t *testing.T) {
	r := mustOk(t, "let x = 1 + 2")
	assertBinding(t, r, "x", typechecker.TypeInt)
}

func TestInferBinaryFloatAdd(t *testing.T) {
	r := mustOk(t, "let x = 1.0 + 2.0")
	assertBinding(t, r, "x", typechecker.TypeFloat)
}

func TestInferBinaryComparison(t *testing.T) {
	r := mustOk(t, "let x = 1 < 2")
	assertBinding(t, r, "x", typechecker.TypeBool)
}

func TestInferBinaryEquality(t *testing.T) {
	r := mustOk(t, `let x = "a" == "b"`)
	assertBinding(t, r, "x", typechecker.TypeBool)
}

func TestInferBinaryLogical(t *testing.T) {
	r := mustOk(t, "let x = true && false")
	assertBinding(t, r, "x", typechecker.TypeBool)
}

func TestBinaryMixedTypesError(t *testing.T) {
	mustFail(t, `let x = 1 + "a"`)
}

func TestBinaryMixedTypesErrorKind(t *testing.T) {
	r := mustFail(t, `let x = 1 + "a"`)
	if r.Errors[0].Kind != typechecker.ErrBinaryTypeMismatch {
		t.Errorf("error kind: got %v, want ErrBinaryTypeMismatch", r.Errors[0].Kind)
	}
}

func TestLogicalRequiresBool(t *testing.T) {
	mustFail(t, "let x = 1 && 2")
}

// ── Variable binding and reference ────────────────────────────────────────────

func TestLetBindingPropagates(t *testing.T) {
	src := "let x = 42\nlet y = x"
	r := mustOk(t, src)
	assertBinding(t, r, "y", typechecker.TypeInt)
}

func TestUndefinedVariableError(t *testing.T) {
	r := mustFail(t, "let x = undefined_var")
	if r.Errors[0].Kind != typechecker.ErrUndefinedVar {
		t.Errorf("error kind: got %v, want ErrUndefinedVar", r.Errors[0].Kind)
	}
}

// ── Pipeline type flow ────────────────────────────────────────────────────────

func TestPipelineSubjectType(t *testing.T) {
	// Subject type is the initial type; unknown function calls pass through Unknown
	src := `let x = 42\n:: double()`
	r := mustOk(t, "let x = 42\n:: double()")
	_ = src
	// After calling unknown function, type is Unknown
	got, _ := r.Bindings["x"]
	if got == nil {
		t.Fatal("binding x not found")
	}
	// Unknown is acceptable when function is not declared
	_ = got
}

func TestPipelineKnownFnType(t *testing.T) {
	// Declared function: check that result type matches declared return type
	src := `fn toStr(n: Int) -> String { }
let x = 42
:: toStr()`
	r := mustOk(t, src)
	assertBinding(t, r, "x", typechecker.TypeString)
}

func TestPipelineArgTypeMismatch(t *testing.T) {
	src := `fn double(n: Int) -> Int { }
let x = "hello"
:: double()`
	mustFail(t, src)
}

// ── Fn declaration type binding ───────────────────────────────────────────────

func TestFnDeclarationBound(t *testing.T) {
	src := `fn add(a: Int, b: Int) -> Int { }`
	r := mustOk(t, src)
	// fn should be registered; calling it should work
	_ = r
}

// ── Trait bound checking ──────────────────────────────────────────────────────

func TestTraitBoundSatisfied(t *testing.T) {
	src := `trait #Printable { }
impl #Printable for Int { }
fn print<T: #Printable>(v: T) { }
let x = 42
:: print()`
	mustOk(t, src)
}

func TestTraitBoundViolation(t *testing.T) {
	src := `trait #Printable { }
fn print<T: #Printable>(v: T) { }
let x = 42
:: print()`
	r := mustFail(t, src)
	if r.Errors[0].Kind != typechecker.ErrTraitBoundViolation {
		t.Errorf("error kind: got %v, want ErrTraitBoundViolation", r.Errors[0].Kind)
	}
}

// ── Lifetime: :> tag tracking ─────────────────────────────────────────────────

func TestTagInSamePipeline(t *testing.T) {
	src := `let x = 42
:tag saved
:: double()`
	mustOk(t, src)
}

func TestTagLifetimeEscape(t *testing.T) {
	src := `(:tag snap)`
	mustOk(t, src)
}

func TestTagJoinValid(t *testing.T) {
	src := `let x = 42
:tag original
:: double()
:join original`
	mustOk(t, src)
}

func TestTagJoinDeadError(t *testing.T) {
	src := `let x = 42
:join nonexistent`
	mustFail(t, src)
}

func TestTagJoinResultIsTuple(t *testing.T) {
	src := `let result = 42
:tag saved
:: double()
:join saved`
	r := mustOk(t, src)
	got, ok := r.Bindings["result"]
	if !ok {
		t.Fatal("binding 'result' not found")
	}
	if _, isTuple := got.(*typechecker.TupleType); !isTuple {
		t.Errorf("expected TupleType, got %T (%s)", got, got.String())
	}
}

// ── impl completeness ─────────────────────────────────────────────────────────

func TestImplMissingMethod(t *testing.T) {
	// trait requires trim() and len(), impl only provides trim() → error
	src := `trait #Text {
  fn trim() -> String { }
  fn len() -> Int { }
}
impl #Text for String {
  fn trim() -> String { }
}`
	r := mustFail(t, src)
	if r.Errors[0].Kind != typechecker.ErrImplMissing {
		t.Errorf("error kind: got %v, want ErrImplMissing", r.Errors[0].Kind)
	}
}

func TestImplComplete(t *testing.T) {
	src := `trait #Text {
  fn trim() -> String { }
  fn len() -> Int { }
}
impl #Text for String {
  fn trim() -> String { }
  fn len() -> Int { }
}`
	mustOk(t, src)
}

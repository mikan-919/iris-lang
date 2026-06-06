package eval_test

import (
	"iris-lang/eval"
	"iris-lang/lexer"
	"iris-lang/parser"
	"testing"
)

// ── helpers ───────────────────────────────────────────────────────────────────

func run(src string) eval.Value {
	prog := parser.New(lexer.New(src)).ParseProgram()
	e := eval.New()
	return e.Exec(prog)
}

func runExpr(src string) eval.Value {
	// Wrap in a let so we can evaluate a single expression
	return run("let __result = " + src).(eval.LetResult).Get("__result")
}

func assertInt(t *testing.T, v eval.Value, want int64) {
	t.Helper()
	iv, ok := v.(*eval.IntValue)
	if !ok {
		t.Fatalf("expected IntValue, got %T (%v)", v, v)
	}
	if iv.V != want {
		t.Errorf("got %d, want %d", iv.V, want)
	}
}

func assertFloat(t *testing.T, v eval.Value, want float64) {
	t.Helper()
	fv, ok := v.(*eval.FloatValue)
	if !ok {
		t.Fatalf("expected FloatValue, got %T (%v)", v, v)
	}
	if fv.V != want {
		t.Errorf("got %f, want %f", fv.V, want)
	}
}

func assertString(t *testing.T, v eval.Value, want string) {
	t.Helper()
	sv, ok := v.(*eval.StringValue)
	if !ok {
		t.Fatalf("expected StringValue, got %T (%v)", v, v)
	}
	if sv.V != want {
		t.Errorf("got %q, want %q", sv.V, want)
	}
}

func assertBool(t *testing.T, v eval.Value, want bool) {
	t.Helper()
	bv, ok := v.(*eval.BoolValue)
	if !ok {
		t.Fatalf("expected BoolValue, got %T (%v)", v, v)
	}
	if bv.V != want {
		t.Errorf("got %v, want %v", bv.V, want)
	}
}

func assertArray(t *testing.T, v eval.Value, wantLen int) *eval.ArrayValue {
	t.Helper()
	av, ok := v.(*eval.ArrayValue)
	if !ok {
		t.Fatalf("expected ArrayValue, got %T (%v)", v, v)
	}
	if len(av.Elements) != wantLen {
		t.Errorf("array len: got %d, want %d", len(av.Elements), wantLen)
	}
	return av
}

func binding(t *testing.T, src, name string) eval.Value {
	t.Helper()
	prog := parser.New(lexer.New(src)).ParseProgram()
	e := eval.New()
	result := e.Exec(prog)
	lr, ok := result.(eval.LetResult)
	if !ok {
		t.Fatalf("expected LetResult, got %T", result)
	}
	v := lr.Get(name)
	if v == nil {
		t.Fatalf("binding %q not found", name)
	}
	return v
}

// ── literal evaluation ────────────────────────────────────────────────────────

func TestEvalInt(t *testing.T) {
	assertInt(t, binding(t, "let x = 42", "x"), 42)
}

func TestEvalNegativeInt(t *testing.T) {
	assertInt(t, binding(t, "let x = -7", "x"), -7)
}

func TestEvalFloat(t *testing.T) {
	assertFloat(t, binding(t, "let x = 3.14", "x"), 3.14)
}

func TestEvalString(t *testing.T) {
	assertString(t, binding(t, `let x = "hello"`, "x"), "hello")
}

func TestEvalBoolTrue(t *testing.T) {
	assertBool(t, binding(t, "let x = true", "x"), true)
}

func TestEvalBoolFalse(t *testing.T) {
	assertBool(t, binding(t, "let x = false", "x"), false)
}

func TestEvalArrayEmpty(t *testing.T) {
	assertArray(t, binding(t, "let x = []", "x"), 0)
}

func TestEvalArrayInts(t *testing.T) {
	av := assertArray(t, binding(t, "let x = [1, 2, 3]", "x"), 3)
	assertInt(t, av.Elements[0], 1)
	assertInt(t, av.Elements[1], 2)
	assertInt(t, av.Elements[2], 3)
}

// ── unary expressions ─────────────────────────────────────────────────────────

func TestEvalUnaryMinus(t *testing.T) {
	assertInt(t, binding(t, "let x = -10", "x"), -10)
}

func TestEvalUnaryNot(t *testing.T) {
	assertBool(t, binding(t, "let x = !true", "x"), false)
	assertBool(t, binding(t, "let x = !false", "x"), true)
}

// ── binary expressions ────────────────────────────────────────────────────────

func TestEvalBinaryAdd(t *testing.T) {
	assertInt(t, binding(t, "let x = 3 + 4", "x"), 7)
}

func TestEvalBinarySub(t *testing.T) {
	assertInt(t, binding(t, "let x = 10 - 3", "x"), 7)
}

func TestEvalBinaryMul(t *testing.T) {
	assertInt(t, binding(t, "let x = 6 * 7", "x"), 42)
}

func TestEvalBinaryDiv(t *testing.T) {
	assertInt(t, binding(t, "let x = 10 / 2", "x"), 5)
}

func TestEvalBinaryMod(t *testing.T) {
	assertInt(t, binding(t, "let x = 10 % 3", "x"), 1)
}

func TestEvalBinaryFloatAdd(t *testing.T) {
	assertFloat(t, binding(t, "let x = 1.5 + 2.5", "x"), 4.0)
}

func TestEvalBinaryStringConcat(t *testing.T) {
	assertString(t, binding(t, `let x = "hello" + " world"`, "x"), "hello world")
}

func TestEvalBinaryEqTrue(t *testing.T) {
	assertBool(t, binding(t, "let x = 1 == 1", "x"), true)
}

func TestEvalBinaryEqFalse(t *testing.T) {
	assertBool(t, binding(t, "let x = 1 == 2", "x"), false)
}

func TestEvalBinaryNeq(t *testing.T) {
	assertBool(t, binding(t, "let x = 1 != 2", "x"), true)
}

func TestEvalBinaryLt(t *testing.T) {
	assertBool(t, binding(t, "let x = 3 < 5", "x"), true)
}

func TestEvalBinaryGt(t *testing.T) {
	assertBool(t, binding(t, "let x = 5 > 3", "x"), true)
}

func TestEvalBinaryAnd(t *testing.T) {
	assertBool(t, binding(t, "let x = true && false", "x"), false)
}

func TestEvalBinaryOr(t *testing.T) {
	assertBool(t, binding(t, "let x = false || true", "x"), true)
}

// ── let binding and variable reference ───────────────────────────────────────

func TestLetBindingRefersBack(t *testing.T) {
	src := "let a = 10\nlet b = a"
	assertInt(t, binding(t, src, "b"), 10)
}

func TestLetBindingChain(t *testing.T) {
	src := "let a = 3\nlet b = a + 4\nlet c = b * 2"
	assertInt(t, binding(t, src, "c"), 14)
}

// ── pipeline with :: ──────────────────────────────────────────────────────────

func TestPipelineOperatorApplication(t *testing.T) {
	// :: * 2  →  subject * 2
	src := "let x = 5\n:: * 2"
	assertInt(t, binding(t, src, "x"), 10)
}

func TestPipelineMultipleSteps(t *testing.T) {
	src := "let x = 3\n:: * 2\n:: + 1"
	assertInt(t, binding(t, src, "x"), 7)
}

func TestPipelineBuiltinTrim(t *testing.T) {
	src := "let x = \"  hello  \"\n:: trim()"
	assertString(t, binding(t, src, "x"), "hello")
}

func TestPipelineBuiltinLower(t *testing.T) {
	src := "let x = \"HELLO\"\n:: lower()"
	assertString(t, binding(t, src, "x"), "hello")
}

func TestPipelineBuiltinUpper(t *testing.T) {
	src := "let x = \"hello\"\n:: upper()"
	assertString(t, binding(t, src, "x"), "HELLO")
}

func TestPipelineBuiltinLen(t *testing.T) {
	src := "let x = \"hello\"\n:: len()"
	assertInt(t, binding(t, src, "x"), 5)
}

func TestPipelineBuiltinSplit(t *testing.T) {
	src := "let x = \"a,b,c\"\n:: split(\",\")"
	av := assertArray(t, binding(t, src, "x"), 3)
	assertString(t, av.Elements[0], "a")
	assertString(t, av.Elements[1], "b")
	assertString(t, av.Elements[2], "c")
}

func TestPipelineBuiltinJoin(t *testing.T) {
	src := "let x = [\"a\", \"b\", \"c\"]\n:: join(\",\")"
	assertString(t, binding(t, src, "x"), "a,b,c")
}

func TestPipelineBuiltinArrayLen(t *testing.T) {
	src := "let x = [1, 2, 3, 4]\n:: len()"
	assertInt(t, binding(t, src, "x"), 4)
}

// ── backbone: :tag and :join ──────────────────────────────────────────────────

func TestTagPassesThrough(t *testing.T) {
	src := "let x = 42\n:tag saved\n:: * 2"
	assertInt(t, binding(t, src, "x"), 84)
}

func TestJoinCreatesTuple(t *testing.T) {
	src := "let x = 10\n:tag original\n:: * 3\n:join original"
	tv, ok := binding(t, src, "x").(*eval.TupleValue)
	if !ok {
		t.Fatalf("expected TupleValue, got %T", binding(t, src, "x"))
	}
	if len(tv.Fields) != 2 {
		t.Fatalf("tuple fields: got %d, want 2", len(tv.Fields))
	}
	assertInt(t, tv.Fields[0], 30) // current subject (10 * 3)
	assertInt(t, tv.Fields[1], 10) // original tag
}

// ── backbone: :or fallback ────────────────────────────────────────────────────

func TestOrFallback(t *testing.T) {
	src := "let x = \"ok\"\n:or \"fallback\""
	assertString(t, binding(t, src, "x"), "ok")
}

// ── backbone: :catch ──────────────────────────────────────────────────────────

func TestCatchOnError(t *testing.T) {
	src := `fn fail() -> Int = error("oops")
let x = 0
:: fail()
:catch 99`
	assertInt(t, binding(t, src, "x"), 99)
}

func TestCatchNotFiredOnSuccess(t *testing.T) {
	src := "let x = 42\n:catch 0"
	assertInt(t, binding(t, src, "x"), 42)
}

// ── backbone: :try ────────────────────────────────────────────────────────────

func TestTryPropagatesError(t *testing.T) {
	src := `fn fail() -> Int = error("bad")
let x = 0
:: fail()
:try
:catch 7`
	assertInt(t, binding(t, src, "x"), 7)
}

// ── user-defined functions ────────────────────────────────────────────────────

func TestUserFnExprBody(t *testing.T) {
	src := `fn double(n: Int) -> Int = n :: * 2
let x = 5
:: double()`
	assertInt(t, binding(t, src, "x"), 10)
}

func TestUserFnBlockBody(t *testing.T) {
	src := `fn add(a: Int, b: Int) -> Int {
  let result = a + b
}
let x = 3
:: add(4)`
	assertInt(t, binding(t, src, "x"), 7)
}

func TestUserFnClosureEnv(t *testing.T) {
	// Function uses a param in binary expr
	src := `fn square(n: Int) -> Int = n :: * n
let x = 6
:: square()`
	assertInt(t, binding(t, src, "x"), 36)
}

// ── match expression ──────────────────────────────────────────────────────────

func TestMatchFirstArm(t *testing.T) {
	src := `let x = "hello"
:: match (
  "hello" :: upper()
  "world" :: lower()
)`
	assertString(t, binding(t, src, "x"), "HELLO")
}

func TestMatchSecondArm(t *testing.T) {
	src := `let x = "world"
:: match (
  "hello" :: upper()
  "world" :: lower()
)`
	assertString(t, binding(t, src, "x"), "world")
}

func TestMatchWildcard(t *testing.T) {
	src := `let x = "other"
:: match (
  "hello" :: upper()
  _ :: trim()
)`
	assertString(t, binding(t, src, "x"), "other")
}

func TestMatchNoArmReturnsNil(t *testing.T) {
	src := `let x = 3
:: match (
  1 :: upper()
  2 :: lower()
)`
	v := binding(t, src, "x")
	if _, ok := v.(*eval.NilValue); !ok {
		t.Errorf("expected NilValue for unmatched match, got %T", v)
	}
}

// ── headless pipeline ─────────────────────────────────────────────────────────

func TestHeadlessPipelineApply(t *testing.T) {
	// A headless pipeline is a Flow value; applying it to a subject runs the steps.
	src := `let flow = (:: * 2 :: + 1)
let x = 5
:: flow()`
	assertInt(t, binding(t, src, "x"), 11)
}

// ── subject placeholder $ ─────────────────────────────────────────────────────

func TestSubjectPlaceholderInCall(t *testing.T) {
	// contains(needle, haystack) — subject as second arg via $
	src := `fn contains(needle: String, haystack: String) -> Bool = false
let x = "world"
:: contains("hello", $)`
	// We just test it runs without error; the fn body returns false for now
	v := binding(t, src, "x")
	if v == nil {
		t.Fatal("expected a value, got nil")
	}
}

// ── :if / :then / :else ───────────────────────────────────────────────────────

func TestIfThenTrueBranch(t *testing.T) {
	src := "let x = 10\n:if true\n:then 100\n:else 200"
	assertInt(t, binding(t, src, "x"), 100)
}

func TestIfThenFalseBranch(t *testing.T) {
	src := "let x = 10\n:if false\n:then 100\n:else 200"
	assertInt(t, binding(t, src, "x"), 200)
}

func TestIfThenNoElsePassThrough(t *testing.T) {
	// Condition is false and there's no :else → subject passes through unchanged
	src := "let x = 42\n:if false\n:then 999"
	assertInt(t, binding(t, src, "x"), 42)
}

func TestIfThenSideEffect(t *testing.T) {
	// :then% — body runs but subject is NOT replaced
	src := "let x = 42\n:if true\n:then% 999"
	assertInt(t, binding(t, src, "x"), 42)
}

func TestIfThenCallBranch(t *testing.T) {
	src := "fn double(n: Int) -> Int = n :: * 2\nlet x = 5\n:if true\n:then double()"
	assertInt(t, binding(t, src, "x"), 10)
}

// ── :while / :then ────────────────────────────────────────────────────────────

func TestWhileCountsDown(t *testing.T) {
	// :while loops until condition is false
	// subject starts at 3, each iteration applies double()
	// condition: subject > 0 (we use a helper fn)
	src := `fn isPositive(n: Int) -> Bool = n > 0
fn decrement(n: Int) -> Int = n + -1
let x = 3
:while isPositive()
:then decrement()`
	// 3 → 2 → 1 → 0 (condition false) → stops at 0
	assertInt(t, binding(t, src, "x"), 0)
}

func TestWhileSideEffect(t *testing.T) {
	// :then% — condition is false from the start, so loop never runs; subject stays
	src := `let x = 3
:while false
:then% 999`
	assertInt(t, binding(t, src, "x"), 3)
}

// ── :catch $alias ─────────────────────────────────────────────────────────────

func TestCatchNamedCatchesError(t *testing.T) {
	src := `fn fail() -> Int = error("bad")
let x = 0
:: fail()
:catch $err 99`
	assertInt(t, binding(t, src, "x"), 99)
}

func TestCatchNamedNotFiredOnSuccess(t *testing.T) {
	src := `let x = 42
:catch $err 0`
	assertInt(t, binding(t, src, "x"), 42)
}

func TestCatchNamedNoAlias(t *testing.T) {
	src := `fn fail() -> Int = error("oops")
let x = 0
:: fail()
:catch 7`
	assertInt(t, binding(t, src, "x"), 7)
}

// ── :await ────────────────────────────────────────────────────────────────────

func TestAwaitPassesThrough(t *testing.T) {
	// :await is treated as sync for now — subject passes through unchanged
	src := "let x = 42\n:await"
	assertInt(t, binding(t, src, "x"), 42)
}

// ── expression statement (top-level pipeline, no let) ─────────────────────────

func TestExpressionStatementRuns(t *testing.T) {
	// Should run without panicking; result is the last expression value
	prog := parser.New(lexer.New(`"hello" :: upper()`)).ParseProgram()
	e := eval.New()
	v := e.Exec(prog)
	_ = v // just check it doesn't panic
}

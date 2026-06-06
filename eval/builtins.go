package eval

import (
	"fmt"
	"strings"
)

// builtinFn is a function callable from Iris source.
type builtinFn func(subject Value, args []Value) Value

// builtins is the global table of built-in functions.
var builtins = map[string]builtinFn{
	// ── String operations ─────────────────────────────────────────────────────
	"trim":  builtinTrim,
	"lower": builtinLower,
	"upper": builtinUpper,
	"len":   builtinLen,
	"split": builtinSplit,
	"join":  builtinJoin,
	// ── Conversion ───────────────────────────────────────────────────────────
	"toString": builtinToString,
	"toInt":    builtinToInt,
	// ── Error constructors ────────────────────────────────────────────────────
	"error": builtinError,
	// ── Debug / IO ───────────────────────────────────────────────────────────
	"print": builtinPrint,
}

func builtinTrim(subject Value, _ []Value) Value {
	s, ok := subject.(*StringValue)
	if !ok {
		return &ErrorValue{"trim: expected String, got " + subject.Type()}
	}
	return &StringValue{strings.TrimSpace(s.V)}
}

func builtinLower(subject Value, _ []Value) Value {
	s, ok := subject.(*StringValue)
	if !ok {
		return &ErrorValue{"lower: expected String, got " + subject.Type()}
	}
	return &StringValue{strings.ToLower(s.V)}
}

func builtinUpper(subject Value, _ []Value) Value {
	s, ok := subject.(*StringValue)
	if !ok {
		return &ErrorValue{"upper: expected String, got " + subject.Type()}
	}
	return &StringValue{strings.ToUpper(s.V)}
}

func builtinLen(subject Value, _ []Value) Value {
	switch s := subject.(type) {
	case *StringValue:
		return &IntValue{int64(len([]rune(s.V)))}
	case *ArrayValue:
		return &IntValue{int64(len(s.Elements))}
	}
	return &ErrorValue{"len: expected String or Array, got " + subject.Type()}
}

func builtinSplit(subject Value, args []Value) Value {
	s, ok := subject.(*StringValue)
	if !ok {
		return &ErrorValue{"split: expected String subject, got " + subject.Type()}
	}
	sep := ","
	if len(args) > 0 {
		if sv, ok := args[0].(*StringValue); ok {
			sep = sv.V
		}
	}
	parts := strings.Split(s.V, sep)
	elems := make([]Value, len(parts))
	for i, p := range parts {
		elems[i] = &StringValue{p}
	}
	return &ArrayValue{elems}
}

func builtinJoin(subject Value, args []Value) Value {
	a, ok := subject.(*ArrayValue)
	if !ok {
		return &ErrorValue{"join: expected Array subject, got " + subject.Type()}
	}
	sep := ""
	if len(args) > 0 {
		if sv, ok := args[0].(*StringValue); ok {
			sep = sv.V
		}
	}
	parts := make([]string, len(a.Elements))
	for i, e := range a.Elements {
		parts[i] = e.String()
	}
	return &StringValue{strings.Join(parts, sep)}
}

func builtinToString(subject Value, _ []Value) Value {
	return &StringValue{subject.String()}
}

func builtinToInt(subject Value, _ []Value) Value {
	switch s := subject.(type) {
	case *IntValue:
		return s
	case *FloatValue:
		return &IntValue{int64(s.V)}
	}
	return &ErrorValue{"toInt: cannot convert " + subject.Type()}
}

func builtinError(_ Value, args []Value) Value {
	msg := "error"
	if len(args) > 0 {
		msg = args[0].String()
	}
	return &ErrorValue{msg}
}

func builtinPrint(subject Value, args []Value) Value {
	if len(args) == 0 {
		fmt.Println(subject.String())
	} else {
		parts := make([]string, 0, len(args)+1)
		parts = append(parts, subject.String())
		for _, a := range args {
			parts = append(parts, a.String())
		}
		fmt.Println(strings.Join(parts, " "))
	}
	return subject
}

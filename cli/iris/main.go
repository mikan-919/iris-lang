// iris is the Iris language CLI.
//
// Usage:
//
//	iris <file>           parse a file and pretty-print its AST
//	iris --lex <file>     print the token stream only
//	iris --repl           start an interactive REPL (read-parse-print)
package main

import (
	"bufio"
	"fmt"
	"iris-lang/eval"
	"iris-lang/lexer"
	"iris-lang/parser"
	"iris-lang/token"
	"os"
	"strings"
)

func main() {
	args := os.Args[1:]

	switch {
	case len(args) == 0:
		repl()
	case args[0] == "--repl":
		repl()
	case args[0] == "--lex" && len(args) == 2:
		lexFile(args[1])
	case args[0] == "--lex":
		die("Usage: iris --lex <file>")
	case args[0] == "--ast" && len(args) == 2:
		parseFile(args[1])
	case args[0] == "--ast":
		die("Usage: iris --ast <file>")
	default:
		runFile(args[0])
	}
}

// ── File modes ────────────────────────────────────────────────────────────────

// runFile is the default mode: parse, type-check, evaluate, print the result.
func runFile(path string) {
	src, err := os.ReadFile(path)
	if err != nil {
		die("cannot read %s: %v", path, err)
	}

	l := lexer.New(string(src))
	prog := parser.New(l).ParseProgram()

	if len(prog.Errors) > 0 {
		for _, e := range prog.Errors {
			fmt.Fprintf(os.Stderr, "parse error: %s\n", e.Error())
		}
		os.Exit(1)
	}

	result := eval.New().Exec(prog)
	fmt.Println(result.String())
}

func parseFile(path string) {
	src, err := os.ReadFile(path)
	if err != nil {
		die("cannot read %s: %v", path, err)
	}

	l := lexer.New(string(src))
	prog := parser.New(l).ParseProgram()

	fmt.Printf("// %s — %d statement(s)\n\n", path, len(prog.Stmts))
	for _, stmt := range prog.Stmts {
		fmt.Println(stmt.String())
	}

	if len(prog.Errors) > 0 {
		fmt.Fprintln(os.Stderr, "\n// parse errors:")
		for _, e := range prog.Errors {
			fmt.Fprintf(os.Stderr, "  %s\n", e.Error())
		}
		os.Exit(1)
	}
}

func lexFile(path string) {
	src, err := os.ReadFile(path)
	if err != nil {
		die("cannot read %s: %v", path, err)
	}

	l := lexer.New(string(src))
	for _, tok := range l.AllTokens() {
		printToken(tok)
	}
}

// ── REPL ──────────────────────────────────────────────────────────────────────

func repl() {
	fmt.Fprintln(os.Stderr, "Iris REPL  (empty line to submit, Ctrl-C to exit, :lex to toggle lex mode)")
	fmt.Fprintln(os.Stderr)

	scanner := bufio.NewScanner(os.Stdin)
	lexMode := false
	var lines []string

	for {
		if len(lines) == 0 {
			fmt.Fprint(os.Stderr, ">>> ")
		} else {
			fmt.Fprint(os.Stderr, "... ")
		}

		if !scanner.Scan() {
			break
		}
		line := scanner.Text()

		switch strings.TrimSpace(line) {
		case ":lex":
			lexMode = !lexMode
			if lexMode {
				fmt.Println("// lex mode on")
			} else {
				fmt.Println("// lex mode off")
			}
			continue
		case ":q", ":quit", ":exit":
			return
		}

		lines = append(lines, line)

		// Blank line submits the accumulated input
		if strings.TrimSpace(line) != "" {
			continue
		}

		src := strings.Join(lines, "\n")
		lines = lines[:0]

		if src == "" {
			continue
		}

		if lexMode {
			evalLex(src)
		} else {
			evalParse(src)
		}
		fmt.Println()
	}
}

func evalLex(src string) {
	for _, tok := range lexer.New(src).AllTokens() {
		if tok.Kind == token.EOF {
			break
		}
		printToken(tok)
	}
}

func evalParse(src string) {
	prog := parser.New(lexer.New(src)).ParseProgram()
	for _, e := range prog.Errors {
		fmt.Fprintf(os.Stderr, "parse error: %s\n", e.Error())
	}
	if len(prog.Errors) > 0 {
		return
	}
	result := eval.New().Exec(prog)
	fmt.Println(result.String())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

func printToken(tok token.Token) {
	fmt.Printf("%-14v %-20q  %d:%d\n", tok.Kind, tok.Lexeme, tok.Line, tok.Col)
}

func die(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
	os.Exit(1)
}

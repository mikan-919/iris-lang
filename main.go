package main

import (
	"fmt"
	"os"

	"iris-lang/lexer"
	"iris-lang/parser"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Println("Usage: go run main.go <source_file>")
		os.Exit(1)
	}

	src, err := os.ReadFile(os.Args[1])
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}

	l := lexer.New(string(src))
	p := parser.New(l)
	prog := p.ParseProgram()

	fmt.Printf("--- %s (%d statements) ---\n", os.Args[1], len(prog.Stmts))
	fmt.Print(prog.String())

	if len(prog.Errors) > 0 {
		fmt.Fprintf(os.Stderr, "\n--- parse errors ---\n")
		for _, e := range prog.Errors {
			fmt.Fprintf(os.Stderr, "  %s\n", e.Error())
		}
	}
}

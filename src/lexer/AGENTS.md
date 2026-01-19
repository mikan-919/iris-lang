# LEXER MODULE

## OVERVIEW
Tokenizes Iris source code into Vec<Token> with line/column tracking.

## STRUCTURE
- `mod.rs` - TokenKind enum (all 27 tokens) + Token struct + Display impl
- `tokenizer.rs` - Lexer struct with tokenize() method, char-by-char parsing
- `tokenizer_tests.rs` - Unit tests for all token patterns

## WHERE TO LOOK
- Entry: `Lexer::new(input).tokenize()` → `Vec<Token>`
- Token definitions: `TokenKind` enum in mod.rs (line 9) - all syntax
- Lexer state: `line`, `column`, `Peekable<Chars>` in tokenizer.rs (line 3-7)
- Operator sequences: multi-char tokens like `=:`, `::`, `:~` (line 80-130)

## CONVENTIONS
- Whitespace/newline: emit Newline token, skip Whitespace
- Multi-char operators: peek ahead for second char (e.g., `:` peeked → check `=` for `=:`)
- String literals: delimited by `"`, support escape sequences
- Line counting: increment on `\n`, reset column to 1
- Column counting: increment on each char except `\n`

## ANTI-PATTERNS
- Don't emit Whitespace tokens (skip silently)
- Don't panic on unknown chars: produce Identifier or handle gracefully
- Don't forget EOF token: must terminate Vec<Token>
- Don't hardcode operator lengths: use peek() for dynamic checking

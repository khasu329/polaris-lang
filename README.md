# Polaris (v0.1)

Polaris is a minimal, statically typed, pattern-rewriting language built around multidimensional heterogeneous tables and fixed-point `while` loops.

## Quick Start

1. Create a new Rust project and place `main.rs` at `src/main.rs`:
   ```bash
   cargo new polaris
   cd polaris
   # Copy main.rs to src/main.rs
   ```
2. Run the built-in test suite:
   ```bash
   cargo test
   ```

## How to Use

Save your Polaris program to a file (for example, `sample.pol`) and run it with `cargo run -- <file>`.

### Interactive Input
Run the program and type your input values directly in the terminal:
```bash
cargo run -- sample.pol
# Type your input directly and press Enter:
6 1 1 2 1 1 1
```
Polaris reads only the tokens/lines required by your `(Input)` declarations and prints the result as soon as the final input is entered.

### Pipe or File Redirection
You can also pass input from a command pipe or a text file:
```bash
# Using a pipe
printf "6 1 1 2 1 1 1\n" | cargo run -- sample.pol

# Using a file
cargo run -- sample.pol < input.txt
```

### Build a Standalone Binary
```bash
cargo build --release
./target/release/polaris sample.pol < input.txt
```

---

## 1. v0.1 Scope
- **Single-file, top-level execution**: No user-defined functions (`fn`), structs (`struct`), or classes (`class`).
- **Explicit static typing**: No type inference or Option types (`?`, `_`). All variables and arrays must declare their types explicitly and are initialized to default values.
- **Minimal multi-value evaluation**: Every expression and rewrite rule evaluates to `(Value, Status)`. In v0.1, `Status` is used internally by `OR(In)` and `AND(In)` to control loop termination.

## 2. Lexical Structure
- **Comments**: `//` to the end of the line.
- **Identifiers**: Start with an ASCII letter or `_`, followed by alphanumeric characters or `_` (e.g., `N`, `Cd`, `a1`).
- **Literals**:
  - Integer: `0`, `1`, `42`, `-10`
  - Float: `3.14`, `-0.5`
  - Char: `'a'`, `'0'`
  - String: `"hello"`
  - Boolean: `true`, `false`
- **Operators & Delimiters**:
  - Assignment & rewriting: `=` (slice assignment, pattern extraction, rewrite rules)
  - Arithmetic: `+`, `-`, `*`, `/`, `%`
  - Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
  - Logical: `&` (AND), `|` (OR), `!` (NOT)
  - Punctuation: `:`, `,`, `...`, `(`, `)`, `[`, `]`, `{`, `}`

## 3. Type System

### Primitive Types
| Type | Description | Default Value |
| :--- | :--- | :--- |
| `Int` | 64-bit signed integer | `0` |
| `Uint` | 64-bit unsigned integer | `0` |
| `Float` | 64-bit floating-point number | `0.0` |
| `Char` | Single Unicode scalar character | `'\0'` |
| `String` | UTF-8 string | `""` |
| `Bool` | Boolean | `false` |

### Array & Table Types
- **1D fixed-length array**: `name: Type[...Len]`
  - Example: `C: Int[...N]`
- **1D variable-length array**: `name: Type[...]`
  - Example: `A: Int[...]` (automatically resizes during rewrites)
- **Heterogeneous multidimensional table**: `name: [Row0Type, Row1Type, ...][...Len]`
  - Example: `Cd: [Int, Int][...N]` (2 rows, N columns)
  - Example: `R: [Char, Int][...]` (2 rows, variable length)

## 4. I/O & Slice Assignment

### Reading Input
- `Input` / `Input(token)` (default): Reads whitespace- or newline-delimited values.
  - `N: Int(Input)`
  - `C: Int[...N](Input)`
- `Input(line)`: Reads one full line per element up to `\n`.
  - `Title: String(Input(line))`
  - `Lines: String[...N](Input(line))`
- `Input(char)`: Reads one character at a time (including spaces, skipping `\r` and `\n`).
  - `Chars: Char[...N](Input(char))`
- `Input(all)`: Reads all remaining characters until EOF.
  - `Text: String(Input(all))`
  - `AllChars: Char[...](Input(all))`

### Printing Output
- Scalar output: `N(Output)`
- Slice output: `Cd[0][...N](Output)` (space-separated elements of row 0)
- Full table output: `R(Output)` (prints each row on its own line)

### Slice Assignment (`=`)
Assigns a scalar (broadcast) or an array slice to a specific row (`[row]`) and length (`[...Len]`):
- Broadcast assignment: `Cd[0][...N] = 0` (sets all N elements in row 0 to 0)
- Slice copy assignment: `Cd[1][...N] = C[...N]` (copies `C` into row 1)

## 5. Control Flow: `while` Loops

`while` takes a continuation condition as its first argument and an optional per-step extraction block as its second argument.

| Form | Syntax | Behavior |
| :--- | :--- | :--- |
| 1-arg (condition) | `while(OR(In)) { ... }` | Continues while `OR(In)` (at least one rule fired) or `AND(In)` (all rules fired) is true. |
| 1-arg (extraction) | `while([x, y] = a(continuous)) { ... }` | Shorthand that scans `a` and loops while any rule in the body fires (`OR(In)`). |
| 2-arg (single) | `while(OR(In), [x, y] = a(continuous)) { ... }` | Executes the extraction in argument 2 on each iteration before evaluating the body. |
| 2-arg (block) | `while(OR(In), { [x] = A(head) [y] = B(head) }) { ... }` | Groups multiple extractions inside `{ ... }` as argument 2. |

### Extraction Modes
- `(continuous)`: Sliding window of adjacent elements (e.g., `[x, y] = a(continuous)`).
- `(any)`: Arbitrary combinations of elements.
- `(head)` / `(tail)`: First or last window only.
- Omitted (`[x] = a`): Iterates over every single element.

## 6. Pattern Rewriting Rules

Inside a `while` block, rewrite rules follow the syntax:
```text
[target_pattern](guard_condition) = [replacement_values]
```

### Semantics
1. **Matching & Binding**: Literals (e.g., `0`) match exact values; identifiers (e.g., `a1`, `c1`) bind the element at that position. Repeated identifiers require equal values.
2. **Guard Evaluation**: The `(guard_condition)` is evaluated as a `Bool`. If omitted, it defaults to `true`.
3. **Partial Update & Auto-Resizing**:
   - Naming only a subset of rows in a multidimensional table updates those rows while leaving other rows untouched (e.g., `[a1, a2](c1 != c2) = [1, 1]`).
   - On variable-length arrays (`[...]`), differing left/right widths automatically expand or shrink the table (e.g., `[[c1, c2], [n1, n2]](c1 == c2) = [[c1], [n1 + n2]]`).
4. **Fixed-Point Termination (`Status`)**:
   - Returns `Status.Executed` (`true`) if at least one rewrite changed a value or resized the array.
   - Returns `Status.NoMatch` (`false`) if no rule matched or if the replacement values are identical to the existing slice.

## 7. Examples

### Test 1: Shortest Distance to a Different Color
```polaris
N: Int(Input)
C: Int[...N](Input)
Cd: [Int, Int][...N]
Cd[0][...N] = 0
Cd[1][...N] = C[...N]

while([[a1, a2], [c1, c2]] = Cd(continuous)) {
  [a1, a2](c1 != c2) = [1, 1]
}

while([[a1, a2], [c1, c2]] = Cd(continuous)) {
  [a1, a2](a1 != 0 & a2 == 0) = [a1, a1 + 1]
  [a1, a2](a1 == 0 & a2 != 0) = [a2 + 1, a2]
}

Cd[0][...N](Output)
```

### Test 2: Run-Length Encoding
```polaris
N: Int(Input)
S: Char[...N](Input)
R: [Char, Int][...]
R[0][...N] = S[...N]
R[1][...N] = 1

while([[c1, c2], [n1, n2]] = R(continuous)) {
  [[c1, c2], [n1, n2]](c1 == c2) = [[c1], [n1 + n2]]
}

R(Output)
```

### Full-Feature Demo (`demo.pol`)
```polaris
Title: String(Input(line))
N: Int(Input)
Nums: Int[...N](Input)
Chars: Char[...N](Input(char))

// Scalar declarations (default-initialized)
Flag: Bool
Pi: Float

Dyn: Int[...]
Dyn[0][...N] = Nums[...N]

while(OR(In), { [x] = Dyn }) {
  [x](x < 0) = [0]
}

while(OR(In), [a1, a2] = Nums(continuous)) {
  [a1, a2](a1 > a2) = [a2, a1]
}

while(OR(In), [h] = Nums(head)) {
  [h](h < 0) = [0]
}
while(OR(In), [t] = Nums(tail)) {
  [t](t > 100) = [100]
}

Tbl: [Int, Char][...N]
Tbl[0][...N] = 0
Tbl[1][...N] = Chars[...N]

while([[d1, d2], [c1, c2]] = Tbl(continuous)) {
  [d1, d2](c1 == c2 & (d1 == 0 | d2 == 0)) = [1, 1]
}

Rle: [Char, Int][...]
Rle[0][...N] = Chars[...N]
Rle[1][...N] = 1

while([[c1, c2], [n1, n2]] = Rle(continuous)) {
  [[c1, c2], [n1, n2]](c1 == c2) = [[c1], [n1 + n2]]
}

Title(Output)
Nums(Output)
Dyn(Output)
Tbl[0][...N](Output)
Rle(Output)
```

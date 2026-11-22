# Polaris (v0.2)

Polaris is a minimal, statically typed language featuring concise `for` loops, structured destructuring assignment, slice views, and multidimensional heterogeneous tables.

## Quick Start

1. Place the Rust source files (`main.rs`, `lib.rs`, `token.rs`, `lexer.rs`, `ast.rs`, `parser.rs`, `interpreter.rs`, `value.rs`) inside `src/` of a Cargo project named `polaris`.
2. Run the test suite:

```bash
cargo test
```

## How to Use

Save your Polaris program to a file (for example, `sample.pol`) and run it with `cargo run -- <file>`.

### Interactive https://www.google.com/search?q=Input

```bash
cargo run -- sample.pol
```

Polaris reads only the tokens required by your `input()` calls and prints results as soon as the final value is entered.

### Pipe or File Redirection

```bash
printf "5 5 3 1 4 2\n" | cargo run -- sample.pol
cargo run -- sample.pol < input.txt
```

### Build a Standalone Binary

```bash
cargo build --release
./target/release/polaris sample.pol < input.txt
```

---

## 1. v0.2 Highlights

- **Lowercase primitive types**: `int`, `uint`, `float`, `char`, `string`, `bool`.
- **Clean array declarations**: `A: int[N]` (fixed length) and `A: int[]` (variable length).
- **Modern I/O**: `input()` and `print()` (legacy `Input`/`Output` modifiers are deprecated).
- **Control flow**: `for i: int in [0..N]` (half-open `[0, N)`), `break`, `continue`, and `if / else` (`while` is deprecated).
- **Destructuring & views**: Parallel swaps `[A[i], A[i + 1]] = [A[i + 1], A[i]]` and slice views `A[0..N]`, `S[1..N]`.

---

## 2. Type System

### Primitive Types

| Type | Description | Default Value |
| :--- | :--- | :--- |
| `int` | 64-bit signed integer | `0` |
| `uint` | 64-bit unsigned integer | `0` |
| `float` | 64-bit floating-point number | `0.0` |
| `char` | Single Unicode scalar character | `'\0'` |
| `string` | UTF-8 string | `""` |
| `bool` | Boolean | `false` |

### Declarations & Multi-Declaration

- Single declaration: `N: int = 0`
- Multi-declaration: `N, M: int` (declares both `N` and `M` as `int`)
- 1D fixed-length array: `A: int[N]`
- 1D variable-length array: `A: int[]`
- Heterogeneous 2D table: `Cd: [int, int][N]` or `R: [char, int][]`

---

## 3. I/O & Indexing

### Reading https://www.google.com/search?q=Input (`input`)

- Inline initialization:
  - `N: int = input()`
  - `A: int[N] = input()`
- Declaration inside `input`:
  - `input(N: int)`
- Reading into existing variables or arrays:
  - `N, M: int` followed by `input(N, M)`
  - `A: int[N]` followed by `input(A)`

### Printing https://www.google.com/search?q=Output (`print`)

- Print one or more values separated by spaces:
  - `print(N)`
  - `print(N, M)`
  - `print(A)`
  - `print(S[0], S[1..N])`
  - `print(Cd[1][0..N])`

### Indexing & Slice Views

- **Element access**: `A[i]`, `S[i]`, `Cd[row][i]`
- **Half-open slice views**: `A[0..N]`, `S[1..N]`, `Cd[row][0..N]`
- **Slice fill assignment**: `Cd[0][0..N] = 0`

---

## 4. Control Flow

### `for` Loops (Half-Open Interval)

`[start..end]` iterates over `start <= i < end`:

```text
for i: int in [0..N] {
  if i == 2 {
    continue
  }
  if i == M {
    break
  }
  sum = sum + i
}
```

### `if / else`

```text
if A[i] > A[i + 1] {
  [A[i], A[i + 1]] = [A[i + 1], A[i]]
} else {
  print(A[i])
}
```

> **Note**: `while` loops from v0.1 remain supported for backward compatibility, but `for` + `if / else` is recommended for all new code.

---

## 5. Examples

### Bubble Sort with Destructuring Assignment

```text
N: int = input()
A: int[N] = input()
flag: bool = true

for step: int in [0..N] {
  if !flag {
    break
  }
  flag = false
  for i: int in [0..N - 1] {
    if A[i] > A[i + 1] {
      [A[i], A[i + 1]] = [A[i + 1], A[i]]
      flag = true
    }
  }
}

print(A);
```

### 2D Table & String Views

```text
input(N: int)
S: string = input()
print(S[0], S[1..N])

C: int[N] = input()
Cd: [int, int][N]
Cd[0][0..N] = 0
for i: int in [0..N] {
  Cd[1][i] = C[i] * 2
}
print(Cd[1][0..N])
```

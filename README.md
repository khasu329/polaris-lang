# Polaris (v0.3)

Polaris is a minimal, statically typed programming language with automatic type inference, lexical block scoping, first-class functions and classes, destructuring assignment, slice views, and multidimensional heterogeneous tables.

## Quick Start

1. Navigate to the `polaris/` Cargo project directory.
2. Run the test suite:

```bash
cargo test
```

## How to Use

Save your Polaris program to a file (for example, `sample.pol`) and run it with `cargo run -- <file>`.

### Interactive

```bash
cargo run -- sample.pol
```

Polaris reads tokens on demand for your `input()` calls and prints results via `print()`.

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

## 1. v0.3 Highlights

- **Removed v0.1 legacy syntax**: Deprecated `while(OR(In))` / `while(AND(In))` extraction loops, guarded rewrite rules (`[a, b](cond) = [...]`), `(Input)` / `(Output)` modifiers, and `[...N]` ellipsis ranges have been completely removed.
- **Pure `while` loops**: Standard `while (condition) { ... }` loops with `break`, `continue`, and `return`.
- **Lexical block scoping**: Every block (`fn`, `if` / `else`, `for`, `while`) owns its own lexical scope with access to enclosing variables and local shadowing.
- **Type inference**: Undeclared assignments (`x = expr`) automatically declare a new variable in the current scope with its type inferred from `expr`. Function return types and `for` loop indices are also inferred when omitted.
- **Functions (`fn`)**: Typed parameters, optional return type (`-> Type`), and `return` statements.
- **Classes (`class`)**: Fields (with optional types and default initializers), methods taking `self`, `ClassName(...)` instantiation, automatic `init` constructor, and automatic `drop` destructor on scope exit.

---

## 2. Type System & Type Inference

### Primitive Types

| Type | Description | Default Value |
| :--- | :--- | :--- |
| `int` | 64-bit signed integer | `0` |
| `uint` | 64-bit unsigned integer | `0` |
| `float` | 64-bit floating-point number | `0.0` |
| `char` | Single Unicode scalar character | `'\0'` |
| `string` | UTF-8 string | `""` |
| `bool` | Boolean | `false` |
| `void` | Unit/empty return type | — |

### Declarations, Inference & Shadowing

- Explicit declaration: `N: int = 0`
- Multi-declaration: `N, M: int`
- Type-inferred declaration (when `x` is not declared in any enclosing scope): `x = 42`, `name = "polaris"`
- Reassignment (when `x` already exists in the current or an outer scope): `x = x + 1`
- Explicit shadowing inside a block: `x: string = "local"`
- 1D fixed-length array: `A: int[N]`
- 1D variable-length array: `A: int[]`
- Heterogeneous 2D table: `Cd: [int, int][N]`

---

## 3. I/O & Indexing

### Reading

- Inline initialization:
  - `N: int = input()`
  - `A: int[N] = input()`
- Declaration inside `input`:
  - `input(N: int)`
- Reading into existing variables or arrays:
  - `N, M: int` followed by `input(N, M)`
  - `A: int[N]` followed by `input(A)`

### Printing & Built-ins

- Print one or more values separated by spaces:
  - `print(N)`
  - `print(N, M)`
  - `print(A)`
  - `print(S[0], S[1..N])`
  - `print(Cd[1][0..N])`
- Length helper: `len(A)` or `len(S)` returns `int`.

### Indexing & Slice Views

- **Element access**: `A[i]`, `S[i]`, `Cd[row][i]`
- **Half-open slice views**: `A[0..N]`, `S[1..N]`, `Cd[row][0..N]`
- **Slice fill assignment**: `Cd[0][0..N] = 0`

---

## 4. Control Flow & Scoping

### `for` Loops (Half-Open Interval)

`[start..end]` iterates over `start <= i < end`:

```text
sum = 0
for i in [0..N] {
  if i == 2 {
    continue
  }
  if i == M {
    break
  }
  sum = sum + i
}
```

### `while` Loops

```text
i = 0
while (i < N) {
  i = i + 1
  if i % 2 == 0 {
    continue
  }
  print(i)
}
```

### `if / else` & Block Scope

```text
x = 10
if x > 5 {
  x = x + 1        // updates outer x
  temp = "scoped"  // inferred local variable inside if-block
  print(x, temp)
}
// temp is out of scope here
```

---

## 5. Functions & Classes

### Functions (`fn`)

```text
fn clamp_add(a: int, b: int, limit: int) -> int {
  total = a + b
  if total > limit {
    return limit
  }
  return total
}

// Return type can also be inferred when `-> Type` is omitted
fn double(x: int) {
  return x * 2
}
```

### Classes (`class`)

```text
class A {
  A: int
  B: int = 5
}

class Counter {
  value: int = 0

  fn init(self, start: int) {
    self.value = start
  }

  fn inc(self, delta: int) -> int {
    self.value = self.value + delta
    return self.value
  }

  fn drop(self) {
    print("dropped", self.value)
  }
}

c = Counter(10)
print(c.inc(5)) // prints 15; c.drop() runs automatically at scope exit
```

---

## 6. Examples

### Bubble Sort with Destructuring Assignment

```text
N: int = input()
A: int[N] = input()
flag = true

for step in [0..N] {
  if !flag {
    break
  }
  flag = false
  for i in [0..N - 1] {
    if A[i] > A[i + 1] {
      [A[i], A[i + 1]] = [A[i + 1], A[i]]
      flag = true
    }
  }
}

print(A)
```

### 2D Table & String Views

```text
input(N: int)
S: string = input()
print(S[0], S[1..N])

C: int[N] = input()
Cd: [int, int][N]
Cd[0][0..N] = 0
for i in [0..N] {
  Cd[1][i] = C[i] * 2
}
print(Cd[1][0..N])
```

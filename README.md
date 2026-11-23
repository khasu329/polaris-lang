# Polaris (v0.4)

**Polaris** is a clean, statically typed programming language written in Rust with automatic type inference, dynamic arrays, unified multidimensional slice views, mutable references (`&`), C++-style generics (`<T>`), a module system (`import`), classes with automatic RAII (`init` / `drop`), and pinpoint error diagnostics.

---

## Table of Contents

1. [Quick Start](#1-quick-start)
2. [Basics: Variables, Type Inference & I/O](#2-basics-variables-type-inference--io)
3. [Control Flow & Lexical Scoping](#3-control-flow--lexical-scoping)
4. [Dynamic Arrays & Multidimensional Slice Views](#4-dynamic-arrays--multidimensional-slice-views)
5. [References (`&T` and `&expr`)](#5-references-t-and-expr)
6. [Functions & Classes (`fn` / `class`)](#6-functions--classes-fn--class)
7. [Generics (C++ Template Style `<T>`)](#7-generics-c-template-style-t)
8. [Modules (`import`)](#8-modules-import)
9. [Error Diagnostics](#9-error-diagnostics)

---

## 1. Quick Start

Move into the `polaris/` directory and run the test suite:

```bash
cd polaris
cargo test
```

Run a Polaris source file (relative `import` paths are resolved from the script's directory):

```bash
# Interactive mode
cargo run -- sample.pol

# Pipe or redirect input
printf "5 5 3 1 4 2\n" | cargo run -- sample.pol
cargo run -- sample.pol < input.txt

# Build standalone release binary
cargo build --release
./target/release/polaris sample.pol < input.txt
```

---

## 2. Basics: Variables, Type Inference & I/O

### Primitive Types

| Type | Description | Default Value |
| :--- | :--- | :--- |
| `int` | 64-bit signed integer | `0` |
| `uint` | 64-bit unsigned integer | `0` |
| `float` | 64-bit floating-point number | `0.0` |
| `char` | Single Unicode character | `'\0'` |
| `string` | UTF-8 string | `""` |
| `bool` | Boolean (`true` / `false`) | `false` |
| `void` | Empty return type | — |

### Declarations & Type Inference

- **Explicit type**: `count: int = 0` or `a, b: int`
- **Automatic inference**: assigning to an undeclared name automatically declares it in the current scope with its inferred type:
  ```text
  x = 42          // inferred as int
  pi = 3.14       // inferred as float
  name = "polaris" // inferred as string
  ```
- **Reassignment**: assigning to an already-declared variable updates it (`x = x + 1`), and type mismatches are caught statically.
- **Shadowing**: adding an explicit `: Type` inside an inner block declares a new local variable without modifying the outer variable.

### Input & Output

```text
N: int = input()        // declare and read one token
input(M: int)           // declare inside input()
A: int[N] = input()     // read N elements into array A

a, b: int
input(a, b)             // read into existing variables

print(N, M, A)          // print space-separated values followed by a newline
```

---

## 3. Control Flow & Lexical Scoping

Every block (`if`, `else`, `for`, `while`, `fn`) has its own lexical scope: inner blocks can read and update outer variables, while variables declared inside a block cannot leak outside.

### `if / else`

```text
if x > 10 {
  print("large")
} else if x > 0 {
  print("positive")
} else {
  print("non-positive")
}
```

### `for` Loop (Half-Open Range `[start..end]`)

Iterates over `start <= i < end` (the loop variable type defaults to `int` when omitted):

```text
sum = 0
for i in [0..5] {
  if i == 2 { continue }
  sum = sum + i
}
print(sum) // 0 + 1 + 3 + 4 = 8
```

### `while` Loop

```text
i = 0
while (i < 5) {
  print(i)
  i = i + 1
}
```

---

## 4. Dynamic Arrays & Multidimensional Slice Views

### Dynamic Arrays & Methods

```text
nums = [10, 20, 30]   // inferred as int[]
nums.push(40)         // [10, 20, 30, 40]
last = nums.pop()     // returns 40; nums is [10, 20, 30]
print(nums.len())     // 3 (len(nums) also works)
nums.clear()          // []

// Empty array literals require a type annotation
tags: string[] = []
tags.push("v0.4")
```

> Assigning an array variable (`b = a`) or passing an array by value (`fn f(a: int[])`) creates an independent copy. To modify the caller's array inside a function, pass a reference (`a: &int[]`) with `&a`.

### Multidimensional Arrays & Heterogeneous Tables

```text
// Fixed 2D matrix: 2 rows of 3 ints
grid: int[2][3]
grid[0][0..3] = 7     // slice fill assignment on row 0

// Dynamic 2D array
mat: int[][] = []
mat.push([1, 2, 3])
mat[0].push(4)
print(mat[0])         // 1 2 3 4

// Heterogeneous 2D row table
table: [int, string][3]
```

### Shared Slice Views (`arr[start..end]`) & Multi-Axis Projection (`A[..end, 0, s..]`)

A slice `A[s..e]` creates a **live view** referencing indices `[s, e)` of the underlying array buffer:

```text
row = [10, 20, 30, 40]
view = row[1..3]      // view into elements [20, 30]
view[0] = 200         // updates row[1] directly!
print(row)            // 10 200 30 40
print(row[1..4])      // 200 30 40
```

For multidimensional arrays, **chained brackets** (`A[..end][0][s..]`) evaluate strictly left-to-right (so `[0]` selects along the first axis `x`), whereas **comma-separated multi-axis indexing** (`A[sel_x, sel_y, sel_z]`) selects or slices each axis simultaneously and produces a live cross-axis view:

```text
// 3D array A[x][y][z]
A: int[3][2][4]

// Chained syntax (left-to-right): keeps (y, z) with x=0
yz_view = A[..2][0][1..]

// Multi-axis projection: fixes y=0 while keeping x in 0..2 and z in 1..4 -> (x, z) view!
xz_view = A[..2, 0, 1..]
xz_view[0, 1] = 999       // writes directly into A[0, 0, 2]
A[1..3, 1, 2..4] = 55     // slice-fill assignment across projected axes

// Column slice on a 2D matrix -> live int[] view
col = mat[0..3, 1]
col[1] = 777              // updates mat[1, 1]
```

- Slice views (including cross-axis projections) can be indexed (`view[i]`, `view[i, j]`), re-sliced (`view[0..1]`), printed (`print(view)`), referenced at leaf elements (`&mat[r, c]`), and passed to functions expecting `T[]` or `T[][]`.
- Destructuring assignment also works for swapping or unpacking:
  ```text
  [A[i], A[i + 1]] = [A[i + 1], A[i]]
  ```

---

## 5. References (`&T` and `&expr`)

Use `&Type` for reference parameters/variables and `&expr` to take a mutable reference to a variable, array element, or object field:

```text
fn add_ten(n: &int) {
  n = n + 10          // automatically writes through to the caller's variable
}

x = 5
add_ten(&x)
print(x)              // 15

// Works with array elements and local aliases too
arr = [1, 2, 3]
r: &int = &arr[1]
r = 99
print(arr)            // 1 99 3
```

**Safety guarantees**:
- Taking `&` of a temporary expression (`&(1 + 2)`, `&f()`) is rejected at type-check time.
- Returning `&T` from a function is disallowed to prevent dangling references.

---

## 6. Functions & Classes (`fn` / `class`)

### Functions

Return types are optional (`-> Type`); when omitted, Polaris infers the return type from `return` statements (or `void` if none).

```text
fn fib(n: int) -> int {
  if n <= 1 { return n }
  return fib(n - 1) + fib(n - 2)
}
```

### Classes, Constructors (`init`), and Destructors (`drop`)

```text
class Counter {
  val: int = 0

  fn init(self, start: int) {
    self.val = start
  }

  fn inc(self, delta: int) -> int {
    self.val = self.val + delta
    return self.val
  }

  fn drop(self) {
    print("Counter dropped at", self.val)
  }
}

if true {
  c = Counter(10)
  print(c.inc(5))     // 15
}                     // c.drop() runs automatically when leaving this block
```

---

## 7. Generics (C++ Template Style `<T>`)

Generic functions and classes use angle-bracket type parameters `<T, U>` and are instantiated per concrete type (like C++ templates), supporting both **explicit** type arguments and **argument-inferred** calls:

```text
fn swap<T>(a: &T, b: &T) {
  tmp = a
  a = b
  b = tmp
}

class Box<T> {
  val: T

  fn init(self, v: T) {
    self.val = v
  }

  fn get(self) -> T {
    return self.val
  }
}

x = 1
y = 2
swap(&x, &y)            // T inferred as int
swap<int>(&x, &y)       // explicit type argument

b1 = Box<int>(42)       // explicit Box<int>
b2 = Box("polaris")     // inferred Box<string>
print(x, y, b1.get(), b2.get())
```

---

## 8. Modules (`import`)

Split your code across multiple `.pol` files and access exported functions, classes, and globals via module qualification:

```text
// math.pol
SCALE = 2

fn double(x: int) -> int {
  return x * SCALE
}

class Vec2 {
  x: int
  y: int
  fn init(self, x: int, y: int) {
    self.x = x
    self.y = y
  }
}
```

```text
// main.pol
import math                   // loads math.pol as `math`
import "utils/geo.pol" as geo // loads relative file path with alias `geo`

v = math.Vec2(3, 4)
print(math.double(v.x), math.SCALE)
```

- Duplicate module loads are cached and executed once.
- Circular imports (`a -> b -> a`) are detected and reported clearly.

---

## 9. Error Diagnostics

Lexical, syntax, static type, module, and runtime errors report the file, line/column, offending source line, a `^` pointer, and actionable hints:

```text
[type error] Type mismatch for Add: int and string
  --> <main>:2:5
  |
2 | b = a + "wrong"
  |     ^
```

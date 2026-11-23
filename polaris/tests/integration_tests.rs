use polaris::{run_source, run_source_with_modules};

// ==========================================
// v0.2 & v0.3 Core Language Features (Preserved in v0.4)
// ==========================================

#[test]
fn test_1_for_and_if_else() {
    let src = r#"
N: int = input()
even_sum: int = 0
odd_sum: int = 0

for i: int in [0..N] {
    if i % 2 == 0 {
        even_sum = even_sum + i
    } else {
        odd_sum = odd_sum + i
    }
}

print(even_sum, odd_sum)
"#;
    let out = run_source(src, "6").unwrap();
    assert_eq!(out.trim(), "6 9");
}

#[test]
fn test_2_break_and_continue() {
    let src = r#"
N, M: int
input(N, M)
sum: int = 0

for i: int in [0..N] {
    if i == 2 {
        continue
    }
    if i == M {
        break
    }
    sum = sum + i
}

print(sum)
"#;
    let out = run_source(src, "10 5").unwrap();
    assert_eq!(out.trim(), "8");
}

#[test]
fn test_3_bubble_sort_destructuring() {
    let src = r#"
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
"#;
    let out = run_source(src, "5 5 3 1 4 2").unwrap();
    assert_eq!(out.trim(), "1 2 3 4 5");
}

#[test]
fn test_4_string_and_array_views() {
    let src = r#"
input(N: int)
S: string = input()
A: int[N] = input()
print(S[0], S[1..N])
print(A[1..N])
"#;
    let out = run_source(src, "4 polaris 10 20 30 40").unwrap();
    assert_eq!(out.trim(), "p ola\n20 30 40");
}

#[test]
fn test_5_table_2d_indexing_and_slice() {
    let src = r#"
N: int = input()
C: int[N] = input()
Cd: [int, int][N]
Cd[0][0..N] = 0
for i: int in [0..N] {
    Cd[1][i] = C[i] * 2
}
print(Cd[0][0..N])
print(Cd[1][0..N])
"#;
    let out = run_source(src, "4 1 2 3 4").unwrap();
    assert_eq!(out.trim(), "0 0 0 0\n2 4 6 8");
}

#[test]
fn test_6_v03_pure_while_loop() {
    let src = r#"
n: int = input()
acc = 0
i = 0

while (i < n) {
    i = i + 1
    if i == 3 {
        continue
    }
    if i > 5 {
        break
    }
    acc = acc + i
}

print(acc)
"#;
    let out = run_source(src, "10").unwrap();
    assert_eq!(out.trim(), "12");
}

#[test]
fn test_7_v03_lexical_block_scoping() {
    let src = r#"
x = 10
y = 100

if true {
    x = x + 5
    y: string = "shadowed"
    z = 42
    print(x, y, z)
}

print(x, y)
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(out.trim(), "15 shadowed 42\n15 100");

    let err_src = r#"
if true {
    z = 42
}
print(z)
"#;
    assert!(run_source(err_src, "").is_err());
}

#[test]
fn test_8_v03_type_inference_and_errors() {
    let src = r#"
a = 21
b = 2.5
c = "polaris"
d = true
sum = 0

for i in [0..5] {
    sum = sum + i
}

fn triple(v: int) {
    return v * 3
}

print(a, b, c, d, sum, triple(4))
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(out.trim(), "21 2.5 polaris true 10 12");

    let mismatch_src = r#"
x = 10
x = "oops"
"#;
    assert!(run_source(mismatch_src, "").is_err());
}

#[test]
fn test_9_v03_functions_and_recursion() {
    let src = r#"
fn mutate_param(B: int, C: string) -> int {
    B = 5
    print(C)
    return B
}

fn fib(n: int) -> int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

x = 99
res = mutate_param(x, "hello")
print(x, res, fib(7))
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(out.trim(), "hello\n99 5 13");

    let bad_ret = r#"
fn A(B: int, C: string) -> int {
    B = 5
    return C
}
print(A(1, "hi"))
"#;
    assert!(run_source(bad_ret, "").is_err());
}

#[test]
fn test_10_v03_classes_fields_methods_and_lifecycle() {
    let src = r#"
class A {
    A: int
    B: int = 5
}

class Counter {
    val: int = 0

    fn init(self, start: int) {
        self.val = start
        print("init", self.val)
    }

    fn inc(self, delta: int) -> int {
        self.val = self.val + delta
        return self.val
    }

    fn drop(self) {
        print("drop", self.val)
    }
}

fn make_counter(start: int) -> Counter {
    c = Counter(start)
    return c
}

a = A()
a.A = 12
print(a.A, a.B)

if true {
    scoped = Counter(10)
    print(scoped.inc(5))
}

kept = make_counter(100)
print(kept.inc(1))
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(
        out.trim(),
        "12 5\ninit 10\n15\ndrop 15\ninit 100\n101\ndrop 101"
    );
}

#[test]
fn test_11_v03_rejects_v01_legacy_syntax() {
    let v01_or_in = r#"
N: int = input()
A: int[N] = input()
while(OR(In), [a1, a2] = A(continuous)) {
    [a1, a2](a1 > a2) = [a2, a1]
}
"#;
    assert!(run_source(v01_or_in, "3 3 2 1").is_err());

    let v01_guard_rewrite = r#"
A: int[2]
[A[0], A[1]](A[0] > A[1]) = [A[1], A[0]]
"#;
    assert!(run_source(v01_guard_rewrite, "").is_err());

    let v01_input_mod = r#"
N: Int(Input)
N(Output)
"#;
    assert!(run_source(v01_input_mod, "5").is_err());

    let v01_ellipsis = r#"
N: int = input()
A: int[...N] = input()
"#;
    assert!(run_source(v01_ellipsis, "2 1 2").is_err());
}

// ==========================================
// v0.4 New Features
// ==========================================

#[test]
fn test_12_v04_dynamic_arrays_and_methods() {
    let src = r#"
nums = [10, 20, 30]
nums.push(40)
last = nums.pop()
print(last, nums.len(), nums)

empty: string[] = []
empty.push("alpha")
empty.push("beta")
print(empty.len(), empty[0], empty[1])
empty.clear()
print(empty.len())

// Copy semantics on value assignment vs & mutation
copy_nums = nums
copy_nums.push(999)
print(nums, copy_nums)
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(
        out.trim(),
        "40 3 10 20 30\n2 alpha beta\n0\n10 20 30 10 20 30 999"
    );

    // Unannotated empty array literal must fail inference
    assert!(run_source("x = []", "").is_err());
    // Popping an empty array must fail cleanly at runtime
    assert!(run_source("x: int[] = []\nx.pop()", "").is_err());
}

#[test]
fn test_13_v04_multidimensional_arrays_and_slice_views() {
    let src = r#"
// Fixed 2D matrix int[R][C]
grid: int[2][3]
grid[0][0..3] = 7
grid[1][0] = 1
grid[1][1] = 2
grid[1][2] = 3
print(grid[0])
print(grid[1][1..3])

// Dynamic 2D array int[][]
mat: int[][] = []
mat.push([10, 20, 30, 40])
mat.push([50, 60])
mat[1].push(70)
print(mat.len(), mat[0].len(), mat[1].len())

// Shared mutable SliceView: modifying view modifies underlying row
view = mat[0][1..3]
print(view.len(), view)
view[0] = 200
sub_view = view[1..2]
sub_view[0] = 300
print(mat[0])

// Passing a slice view into a function expecting int[]
fn sum_arr(arr: int[]) -> int {
    total = 0
    for i in [0..arr.len()] {
        total = total + arr[i]
    }
    return total
}
print(sum_arr(mat[0][1..4]))
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(
        out.trim(),
        "7 7 7\n2 3\n2 4 3\n2 20 30\n10 200 300 40\n540"
    );

    // Resizing a SliceView directly must be rejected
    let slice_push_err = r#"
a = [1, 2, 3, 4]
v = a[1..3]
v.push(9)
"#;
    assert!(run_source(slice_push_err, "").is_err());
}

#[test]
fn test_14_v04_generics_cpp_template_style() {
    let src = r#"
fn add_vals<T>(a: T, b: T) -> T {
    return a + b
}

class Box<T> {
    val: T

    fn init(self, v: T) {
        self.val = v
    }

    fn get(self) -> T {
        return self.val
    }

    fn set(self, v: T) {
        self.val = v
    }
}

class Pair<A, B> {
    first: A
    second: B

    fn init(self, a: A, b: B) {
        self.first = a
        self.second = b
    }

    fn make_tagged<Tag>(self, tag: Tag) -> string {
        return tag
    }
}

print(add_vals<int>(3, 4))
print(add_vals("po", "laris"))

b1 = Box<int>(42)
b2 = Box("hello")
b1.set(100)
print(b1.get(), b2.get())

p = Pair<int, string>(7, "seven")
print(p.first, p.second, p.make_tagged<string>("ok"))
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(out.trim(), "7\npolaris\n100 hello\n7 seven ok");

    // Template error: bool does not support `+` when instantiated as add_vals<bool>
    let bad_template = r#"
fn add_vals<T>(a: T, b: T) -> T {
    return a + b
}
print(add_vals(true, false))
"#;
    assert!(run_source(bad_template, "").is_err());
}

#[test]
fn test_15_v04_references_and_safety() {
    let src = r#"
fn swap<T>(a: &T, b: &T) {
    tmp = a
    a = b
    b = tmp
}

fn append_val(arr: &int[], v: int) {
    arr.push(v)
}

x = 10
y = 20
swap(&x, &y)
print(x, y)

arr = [5, 1, 4, 2, 3]
swap(&arr[0], &arr[4])
append_val(&arr, 99)
print(arr)

class Holder {
    score: int = 0
}
h = Holder()
swap(&h.score, &x)
print(h.score, x)

// Local reference alias
r: &int = &y
r = r + 5
print(y)
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(out.trim(), "20 10\n3 1 4 2 5 99\n20 0\n15");

    // Taking & of rvalue must fail static checks
    assert!(run_source("fn f(x: &int) {}\nf(&(1 + 2))", "").is_err());

    // Passing non-ref to &T parameter must fail static checks
    assert!(run_source("fn f(x: &int) {}\na = 5\nf(a)", "").is_err());

    // Returning &T from function must fail static checks
    assert!(run_source("fn bad() -> &int { x = 1\n return &x }", "").is_err());
}

#[test]
fn test_16_v04_modules_and_circular_detection() {
    let main_src = r#"
import math
import "utils/geo.pol" as geo

p = geo.make_point(3, 4)
print(p.x, p.y, geo.norm_sq(p))
print(math.add(10, math.FACTOR))

b = math.Wrap<string>("module-ok")
print(b.val)
"#;

    let math_pol = r#"
FACTOR = 5

fn add(a: int, b: int) -> int {
    return a + b
}

class Wrap<T> {
    val: T
    fn init(self, v: T) {
        self.val = v
    }
}
"#;

    let geo_pol = r#"
import "../math.pol" as m

class Point {
    x: int
    y: int
    fn init(self, x: int, y: int) {
        self.x = x
        self.y = y
    }
}

fn make_point(x: int, y: int) -> Point {
    return Point(x, y)
}

fn norm_sq(p: Point) -> int {
    return m.add(p.x * p.x, p.y * p.y)
}
"#;

    let out = run_source_with_modules(
        main_src,
        "",
        vec![("math.pol", math_pol), ("utils/geo.pol", geo_pol)],
    )
    .unwrap();
    assert_eq!(out.trim(), "3 4 25\n15\nmodule-ok");

    // Circular import detection
    let circ_a = "import b\n";
    let circ_b = "import a\n";
    let err = run_source_with_modules(
        "import a\n",
        "",
        vec![("a.pol", circ_a), ("b.pol", circ_b)],
    )
    .unwrap_err();
    assert!(err.contains("Circular import"));
}

#[test]
fn test_17_v04_improved_error_messages_with_spans() {
    let bad_type_src = "a = 10\nb = a + \"wrong\"\n";
    let err = run_source(bad_type_src, "").unwrap_err();
    assert!(err.contains("2:"), "Expected line 2 in error:\n{}", err);
    assert!(err.contains("^"), "Expected caret pointer in error:\n{}", err);
    assert!(
        err.contains("b = a + \"wrong\""),
        "Expected source line snippet in error:\n{}",
        err
    );

    let bad_parse_src = "x = (\n";
    let parse_err = run_source(bad_parse_src, "").unwrap_err();
    assert!(parse_err.contains("parse error"), "Got:\n{}", parse_err);
    assert!(parse_err.contains("^"), "Got:\n{}", parse_err);

    let oob_src = "arr = [1, 2]\nprint(arr[5])\n";
    let oob_err = run_source(oob_src, "").unwrap_err();
    assert!(oob_err.contains("runtime error"), "Got:\n{}", oob_err);
    assert!(oob_err.contains("2:"), "Expected line 2 in error:\n{}", oob_err);
    assert!(oob_err.contains("out of bounds"), "Got:\n{}", oob_err);
}

#[test]
fn test_18_v04_multi_axis_projected_views() {
    let src = r#"
// Build a 3D array A[x][y][z] of shape 3x2x4
A: int[3][2][4]
for x in [0..3] {
    for y in [0..2] {
        for z in [0..4] {
            A[x, y, z] = x * 100 + y * 10 + z
        }
    }
}

// 1) Multi-axis (x, z) cross-axis projection: fix y=0, keep x in 0..2 and z in 1..4 -> shape (2, 3)
xz_view = A[..2, 0, 1..]
print(xz_view.len(), xz_view[0].len(), xz_view[1][2])
print(xz_view)

// Mutating through multi-axis view updates A in-place (both v[xi, zi] and v[xi][zi])
xz_view[0, 1] = 999
xz_view[1][2] = 888
print(A[0, 0, 2], A[1, 0, 3])

// Compare with left-to-right chained syntax: A[..2][0][1..]
// A[..2] keeps 3D (x in 0..2), [0] selects x=0 -> 2D (y, z), [1..] keeps y in 1..2 -> shape (1, 4)
yz_chained = A[..2][0][1..]
print(yz_chained.len(), yz_chained[0].len(), yz_chained[0, 0])

// 2) 2D Column slice view mat[0..R, col]
mat: int[3][3]
for r in [0..3] {
    for c in [0..3] {
        mat[r, c] = r * 10 + c
    }
}
col1 = mat[0..3, 1]
print(col1)
col1[1] = 777
print(mat[1, 1])

// Pass column SliceView into function expecting int[]
fn sum_1d(arr: int[]) -> int {
    total = 0
    for i in [0..arr.len()] {
        total = total + arr[i]
    }
    return total
}
print(sum_1d(mat[0..3, 1]))

// 3) (x, y) cross-axis projection: fix z=2
xy_view = A[..2, .., 2]
print(xy_view[0, 0], xy_view[1, 1])

// 4) Taking mutable reference '&' to multi-indexed element and projected view element
fn swap_int(a: &int, b: &int) {
    tmp = a
    a = b
    b = tmp
}
swap_int(&mat[0, 2], &col1[2])
print(mat[0, 2], mat[2, 1])

// 5) Multi-axis slice fill assignment
mat[1.., 0] = -5
print(mat[0, 0], mat[1, 0], mat[2, 0])

// Slice fill across 3D cross-axis view A[1..3, 1, 2..4] = 55
A[1..3, 1, 2..4] = 55
print(A[1, 1, 2], A[2, 1, 3], A[1, 1, 1])
"#;
    let out = run_source(src, "").unwrap();
    assert_eq!(
        out.trim(),
        "2 3 103\n1 2 3 101 102 103\n999 888\n1 4 10\n1 11 21\n777\n799\n999 112\n21 2\n0 -5 -5\n55 55 111"
    );

    // Too many dimensions in multi-index must fail static type checking
    assert!(run_source("a = [1, 2, 3]\nx = a[0, 1]", "").is_err());

    // Taking '&' of a range slice in MultiIndex must fail static checks
    assert!(run_source("m: int[2][2]\nfn f(p: &int[]) {}\nf(&m[0..2, 0])", "").is_err());
}


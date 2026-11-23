use polaris::run_source;

// ==========================================
// v0.2 Core Language Features (Preserved in v0.3)
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

// ==========================================
// v0.3 New Features
// ==========================================

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
    // 1 + 2 + (skip 3) + 4 + 5 = 12
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

    // z declared inside `if` must not be visible outside
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

    // Reassigning a different type to an inferred variable must fail
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

    // Returning wrong type from a typed function must fail
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

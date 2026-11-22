use polaris::run_source;

#[test]
fn test_1_shortest_distance() {
    let src = r#"
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
"#;
    let out = run_source(src, "6 1 1 2 1 1 1").unwrap();
    assert_eq!(out.trim(), "2 1 1 1 2 3");
}

#[test]
fn test_2_run_length_encoding() {
    let src = r#"
N: Int(Input)
S: Char[...N](Input)
R: [Char, Int][...]
R[0][...N] = S[...N]
R[1][...N] = 1

while([[c1, c2], [n1, n2]] = R(continuous)) {
  [[c1, c2], [n1, n2]](c1 == c2) = [[c1], [n1 + n2]]
}

R(Output)
"#;
    let out = run_source(src, "7 a a a b b c a").unwrap();
    assert_eq!(out.trim(), "a b c a\n3 2 1 1");
}

#[test]
fn test_3_bubble_sort_or_in() {
    let src = r#"
N: Int(Input)
A: Int[...N](Input)
while(OR(In),[a1,a2]=A(continous)){
    [a1,a2](a1>a2)=[a2,a1]
}A(Output)
"#;
    let out = run_source(src, "5 5 3 1 4 2").unwrap();
    assert_eq!(out.trim(), "1 2 3 4 5");
}

#[test]
fn test_4_input_modes() {
    let src_line = r#"
N: Int(Input)
L: String[...N](Input(line))
L(Output)
"#;
    let out_line = run_source(
        src_line,
        "2
hello world
foo bar
",
    )
    .unwrap();
    assert_eq!(
        out_line.trim(),
        "hello world
foo bar"
    );

    let src_char = r#"
N: Int(Input)
C: Char[...N](Input(char))
C(Output)
"#;
    let out_char = run_source(
        src_char, "5
a bcd
",
    )
    .unwrap();
    assert_eq!(out_char.trim(), "a   b c d");

    let src_all = r#"
C: Char[...](Input(all))
C(Output)
"#;
    let out_all = run_source(src_all, "hi!").unwrap();
    assert_eq!(out_all.trim(), "h i !");
}

#[test]
fn test_5_new_input_output_syntax() {
    let src = r#"
input(N, M: int)
A: int[...N]
input(A)
output(N)
output(M)
output(A)
"#;
    let out = run_source(src, "3 10 100 200 300").unwrap();
    assert_eq!(out.trim(), "3\n10\n100 200 300");
}

#[test]
fn test_6_separate_decl_and_input() {
    let src = r#"
N: int
input(N)
X, Y: int
input(X, Y)
output(N)
output(X)
output(Y)
"#;
    let out = run_source(src, "1 2 3").unwrap();
    assert_eq!(out.trim(), "1\n2\n3");
}

#[test]
fn test_7_bubble_sort_new_syntax() {
    let src = r#"
input(N: int)
A: int[...N]
input(A)
while(OR(In), [a1, a2] = A(continuous)) {
    [a1, a2](a1 > a2) = [a2, a1]
}
output(A)
"#;
    let out = run_source(src, "5 5 3 1 4 2").unwrap();
    assert_eq!(out.trim(), "1 2 3 4 5");
}

// ==========================================
// v0.2 テストスイート
// ==========================================

#[test]
fn test_8_v02_for_and_if_else() {
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
    // 0..6 => even: 0+2+4=6, odd: 1+3+5=9
    let out = run_source(src, "6").unwrap();
    assert_eq!(out.trim(), "6 9");
}

#[test]
fn test_9_v02_break_and_continue() {
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
    // 0 + 1 + (skip 2) + 3 + 4 + (break at 5) = 8
    let out = run_source(src, "10 5").unwrap();
    assert_eq!(out.trim(), "8");
}

#[test]
fn test_10_v02_bubble_sort_destructuring() {
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
fn test_11_v02_string_and_array_views() {
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
fn test_12_v02_table_2d_indexing_and_slice() {
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

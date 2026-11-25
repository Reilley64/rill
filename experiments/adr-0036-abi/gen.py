"""Writes cases.ll and cases.c: the same values built and checked by LLVM IR and by C, across every calling shape."""

TYPES = {
    "S2": dict(
        c="typedef struct { void *p; uint64_t n; } S2;",
        ll="{ ptr, i64 }",
        size=16,
        words=[(0, "ptr", "p", 0x1111), (8, "i64", "n", 0x2222)],
    ),
    "S3": dict(
        c="typedef struct { void *p; uint64_t n; uint64_t c; } S3;",
        ll="{ ptr, i64, i64 }",
        size=24,
        words=[(0, "ptr", "p", 0x1111), (8, "i64", "n", 0x2222), (16, "i64", "c", 0x3333)],
    ),
    "E24": dict(
        c="typedef struct { uint8_t tag; uint64_t w[2]; } E24;",
        ll="{ i8, [2 x i64] }",
        size=24,
        words=[(0, "i8", "tag", 1), (8, "i64", "w[0]", 0x4444), (16, "i64", "w[1]", 0x5555)],
    ),
    "E56": dict(
        c="typedef struct { uint8_t tag; uint64_t w[6]; } E56;",
        ll="{ i8, [6 x i64] }",
        size=56,
        words=[(0, "i8", "tag", 2)] + [(8 * i, "i64", f"w[{i - 1}]", 0x60 + i) for i in range(1, 7)],
    ),
}

counter = [0]


def fresh(prefix="t"):
    counter[0] += 1
    return f"%{prefix}{counter[0]}"


def ll_fill(ptr, t):
    out = []
    for off, ty, _, val in TYPES[t]["words"]:
        g = fresh("g")
        out.append(f"  {g} = getelementptr i8, ptr {ptr}, i64 {off}")
        v = f"inttoptr (i64 {val} to ptr)" if ty == "ptr" else str(val)
        out.append(f"  store {ty} {v}, ptr {g}")
    return out


def ll_check(ptr, t):
    out = []
    acc = "true"
    for off, ty, _, val in TYPES[t]["words"]:
        g, x, c, n = fresh("g"), fresh("x"), fresh("c"), fresh("n")
        out.append(f"  {g} = getelementptr i8, ptr {ptr}, i64 {off}")
        out.append(f"  {x} = load {ty}, ptr {g}")
        if ty == "ptr":
            p = fresh("p")
            out.append(f"  {p} = ptrtoint ptr {x} to i64")
            out.append(f"  {c} = icmp eq i64 {p}, {val}")
        else:
            out.append(f"  {c} = icmp eq {ty} {x}, {val}")
        out.append(f"  {n} = and i1 {acc}, {c}")
        acc = n
    r = fresh("r")
    out.append(f"  {r} = zext i1 {acc} to i32")
    return out, r


def buf(t):
    a = fresh("buf")
    return a, [
        f"  {a} = alloca {TYPES[t]['ll']}, align 8",
        f"  call void @llvm.memset.p0.i64(ptr {a}, i8 0, i64 {TYPES[t]['size']}, i1 false)",
    ]


def fn(header, body):
    return header + " {\nentry:\n" + "\n".join(body) + "\n}\n"


ll = ["declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)", ""]
cc = []
cases = []  # (name, directions)
CC = []

# C side: type definitions, pattern builders and checkers.
for t, d in TYPES.items():
    CC.append(d["c"])
    sets, tests = [], []
    for _, ty, name, val in d["words"]:
        cast = "(void *)" if ty == "ptr" else ("(uint8_t)" if ty == "i8" else "(uint64_t)")
        sets.append(f"s.{name} = {cast}{val:#x};")
        tests.append(f"s->{name} == {cast}{val:#x}")
    CC.append(f"static {t} pat_{t}(void) {{ {t} s; memset(&s, 0, sizeof s); " + " ".join(sets) + f" return s; }}")
    CC.append(f"static int ok_{t}(const {t} *s) {{ return " + " && ".join(tests) + "; }")
    CC.append("")

# make, first-class return.
for t, d in TYPES.items():
    ty = d["ll"]
    a, pre = buf(t)
    v = fresh("v")
    ll.append(fn(f"define {ty} @ir_make_fc_{t}()", pre + ll_fill(a, t) + [f"  {v} = load {ty}, ptr {a}", f"  ret {ty} {v}"]))
    CC.append(f"{t} c_make_fc_{t}(void) {{ return pat_{t}(); }}")
    ll.append(f"declare {ty} @c_make_fc_{t}()")
    a, pre = buf(t)
    r = fresh("rv")
    chk, res = ll_check(a, t)
    ll.append(fn(f"define i32 @ir_call_make_fc_{t}()", [f"  {r} = call {ty} @c_make_fc_{t}()"] + pre + [f"  store {ty} {r}, ptr {a}"] + chk + [f"  ret i32 {res}"]))
    cases.append((f"make_fc_{t}", "AB"))

# make, sret return.
for t, d in TYPES.items():
    ty = d["ll"]
    ll.append(fn(f"define void @ir_make_sret_{t}(ptr sret({ty}) align 8 %out)", ll_fill("%out", t) + ["  ret void"]))
    CC.append(f"{t} c_make_sret_{t}(void) {{ return pat_{t}(); }}")
    ll.append(f"declare void @c_make_sret_{t}(ptr sret({ty}) align 8)")
    a, pre = buf(t)
    chk, res = ll_check(a, t)
    ll.append(fn(f"define i32 @ir_call_make_sret_{t}()", pre + [f"  call void @c_make_sret_{t}(ptr sret({ty}) align 8 {a})"] + chk + [f"  ret i32 {res}"]))
    cases.append((f"make_sret_{t}", "AB"))

# make, plain out-pointer (no sret attribute).
for t, d in TYPES.items():
    ty = d["ll"]
    for name in (f"ir_make_out_{t}", f"ir_make_out_as_sret_{t}"):
        ll.append(fn(f"define void @{name}(ptr %out)", ll_fill("%out", t) + ["  ret void"]))
    CC.append(f"void c_make_out_{t}({t} *out) {{ *out = pat_{t}(); }}")
    ll.append(f"declare void @c_make_out_{t}(ptr)")
    a, pre = buf(t)
    chk, res = ll_check(a, t)
    ll.append(fn(f"define i32 @ir_call_make_out_{t}()", pre + [f"  call void @c_make_out_{t}(ptr {a})"] + chk + [f"  ret i32 {res}"]))
    cases.append((f"make_out_{t}", "AB"))
    cases.append((f"make_out_as_sret_{t}", "A"))

# take, first-class parameter.
for t, d in TYPES.items():
    ty = d["ll"]
    a, pre = buf(t)
    chk, res = ll_check(a, t)
    ll.append(fn(f"define i32 @ir_take_fc_{t}({ty} %v)", pre + [f"  store {ty} %v, ptr {a}"] + chk + [f"  ret i32 {res}"]))
    CC.append(f"int c_take_fc_{t}({t} v) {{ return ok_{t}(&v); }}")
    ll.append(f"declare i32 @c_take_fc_{t}({ty})")
    a, pre = buf(t)
    v = fresh("v")
    r = fresh("rv")
    ll.append(fn(f"define i32 @ir_call_take_fc_{t}()", pre + ll_fill(a, t) + [f"  {v} = load {ty}, ptr {a}", f"  {r} = call i32 @c_take_fc_{t}({ty} {v})", f"  ret i32 {r}"]))
    cases.append((f"take_fc_{t}", "AB"))

# take, split into scalar words.
for t in ("S2", "S3"):
    d = TYPES[t]
    params = ", ".join(f"{ty} %w{i}" for i, (_, ty, _, _) in enumerate(d["words"]))
    a, pre = buf(t)
    stores = []
    for i, (off, ty, _, _) in enumerate(d["words"]):
        g = fresh("g")
        stores += [f"  {g} = getelementptr i8, ptr {a}, i64 {off}", f"  store {ty} %w{i}, ptr {g}"]
    chk, res = ll_check(a, t)
    ll.append(fn(f"define i32 @ir_take_split_{t}({params})", pre + stores + chk + [f"  ret i32 {res}"]))
    cparams = ", ".join(("void *" if ty == "ptr" else "uint64_t ") + f"w{i}" for i, (_, ty, _, _) in enumerate(d["words"]))
    assigns = " ".join(f"s.{name} = " + ("(void *)" if ty == "ptr" else "") + f"w{i};" for i, (_, ty, name, _) in enumerate(d["words"]))
    CC.append(f"int c_take_split_{t}({cparams}) {{ {t} s; memset(&s, 0, sizeof s); {assigns} return ok_{t}(&s); }}")
    ll.append(f"declare i32 @c_take_split_{t}(" + ", ".join(ty for _, ty, _, _ in d["words"]) + ")")
    a, pre = buf(t)
    loads, args = [], []
    for off, ty, _, _ in d["words"]:
        g, x = fresh("g"), fresh("x")
        loads += [f"  {g} = getelementptr i8, ptr {a}, i64 {off}", f"  {x} = load {ty}, ptr {g}"]
        args.append(f"{ty} {x}")
    r = fresh("rv")
    ll.append(fn(f"define i32 @ir_call_take_split_{t}()", pre + ll_fill(a, t) + loads + [f"  {r} = call i32 @c_take_split_{t}({', '.join(args)})", f"  ret i32 {r}"]))
    cases.append((f"take_split_{t}", "AB"))

# take, LLVM byval.
for t in ("S3", "E56"):
    ty = TYPES[t]["ll"]
    chk, res = ll_check("%p", t)
    ll.append(fn(f"define i32 @ir_take_byval_{t}(ptr byval({ty}) align 8 %p)", chk + [f"  ret i32 {res}"]))
    CC.append(f"int c_take_byval_{t}({t} v) {{ return ok_{t}(&v); }}")
    ll.append(f"declare i32 @c_take_byval_{t}(ptr byval({ty}) align 8)")
    a, pre = buf(t)
    r = fresh("rv")
    ll.append(fn(f"define i32 @ir_call_take_byval_{t}()", pre + ll_fill(a, t) + [f"  {r} = call i32 @c_take_byval_{t}(ptr byval({ty}) align 8 {a})", f"  ret i32 {r}"]))
    cases.append((f"take_byval_{t}", "AB"))

# take, pointer to caller storage.
for t in ("E24", "E56"):
    chk, res = ll_check("%p", t)
    ll.append(fn(f"define i32 @ir_take_ptr_{t}(ptr %p)", chk + [f"  ret i32 {res}"]))
    CC.append(f"int c_take_ptr_{t}(const {t} *p) {{ return ok_{t}(p); }}")
    ll.append(f"declare i32 @c_take_ptr_{t}(ptr)")
    a, pre = buf(t)
    r = fresh("rv")
    ll.append(fn(f"define i32 @ir_call_take_ptr_{t}()", pre + ll_fill(a, t) + [f"  {r} = call i32 @c_take_ptr_{t}(ptr {a})", f"  ret i32 {r}"]))
    cases.append((f"take_ptr_{t}", "AB"))

# Tiny functions whose assembly shows where each shape travels.
for t, d in TYPES.items():
    ty = d["ll"]
    v = fresh("v")
    ll.append(fn(f"define {ty} @probe_ret_fc_{t}(ptr %p)", [f"  {v} = load {ty}, ptr %p", f"  ret {ty} {v}"]))
    ll.append(fn(f"define void @probe_ret_sret_{t}(ptr sret({ty}) align 8 %o, ptr %p)", [f"  call void @llvm.memcpy.p0.p0.i64(ptr %o, ptr %p, i64 {d['size']}, i1 false)", "  ret void"]))
    ll.append(fn(f"define void @probe_arg_fc_{t}({ty} %v, ptr %q)", [f"  store {ty} %v, ptr %q", "  ret void"]))
    CC.append(f"{t} c_probe_ret_{t}(const {t} *p) {{ return *p; }}")
    CC.append(f"void c_probe_arg_{t}({t} v, {t} *q) {{ *q = v; }}")
ll.insert(1, "declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)")

# Layout the enum rule relies on: the tag, then the payload at its natural alignment.
LAYOUT = [
    ("E24", "{ i8, [2 x i64] }", 24, 8),
    ("E56", "{ i8, [6 x i64] }", 56, 8),
    ("E16", "{ i8, [1 x i64] }", 16, 8),
    ("E2", "{ i8, [1 x i8] }", 2, 1),
    ("E8", "{ i8, [1 x i32] }", 8, 4),
]
body = []
for i, (name, ty, size, off) in enumerate(LAYOUT):
    s = f"ptrtoint (ptr getelementptr ({ty}, ptr null, i32 1) to i64)"
    o = f"ptrtoint (ptr getelementptr ({ty}, ptr null, i32 0, i32 1) to i64)"
    body += [f"  %s{i} = getelementptr i64, ptr %out, i64 {2 * i}", f"  store i64 {s}, ptr %s{i}",
             f"  %o{i} = getelementptr i64, ptr %out, i64 {2 * i + 1}", f"  store i64 {o}, ptr %o{i}"]
ll.append(fn("define void @ir_layout(ptr %out)", body + ["  ret void"]))
cc_layout = ["void c_layout_expect(uint64_t *out) {"]
for i, (name, ty, size, off) in enumerate(LAYOUT):
    cc_layout.append(f"  out[{2 * i}] = {size}; out[{2 * i + 1}] = {off};")
cc_layout.append("}")
names = [n for n, *_ in LAYOUT]

header = """#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

"""
with open("cases.ll", "w") as f:
    f.write("\n".join(ll))

# Harness: every case runs in its own process, chosen by run.sh.
ext = []
cases_a = []
for t in TYPES:
    ext += [f"{t} ir_make_fc_{t}(void);", f"int ir_call_make_fc_{t}(void);"]
    cases_a.append((f"make_fc_{t}", f"{t} r = ir_make_fc_{t}(); return ok_{t}(&r);"))
    ext += [f"{t} ir_make_sret_{t}(void);", f"int ir_call_make_sret_{t}(void);"]
    cases_a.append((f"make_sret_{t}", f"{t} r = ir_make_sret_{t}(); return ok_{t}(&r);"))
    ext += [f"void ir_make_out_{t}({t} *out);", f"{t} ir_make_out_as_sret_{t}(void);", f"int ir_call_make_out_{t}(void);"]
    cases_a.append((f"make_out_{t}", f"{t} r; memset(&r, 0, sizeof r); ir_make_out_{t}(&r); return ok_{t}(&r);"))
    cases_a.append((f"make_out_as_sret_{t}", f"{t} r = ir_make_out_as_sret_{t}(); return ok_{t}(&r);"))
    ext += [f"int ir_take_fc_{t}({t} v);", f"int ir_call_take_fc_{t}(void);"]
    cases_a.append((f"take_fc_{t}", f"{t} v = pat_{t}(); return ir_take_fc_{t}(v);"))
for t in ("S2", "S3"):
    words = TYPES[t]["words"]
    params = ", ".join(("void *" if ty == "ptr" else "uint64_t") for _, ty, _, _ in words)
    args = ", ".join(("(void *)" if ty == "ptr" else "(uint64_t)") + f"{val:#x}" for _, ty, _, val in words)
    ext += [f"int ir_take_split_{t}({params});", f"int ir_call_take_split_{t}(void);"]
    cases_a.append((f"take_split_{t}", f"return ir_take_split_{t}({args});"))
for t in ("S3", "E56"):
    ext += [f"int ir_take_byval_{t}({t} v);", f"int ir_call_take_byval_{t}(void);"]
    cases_a.append((f"take_byval_{t}", f"{t} v = pat_{t}(); return ir_take_byval_{t}(v);"))
for t in ("E24", "E56"):
    ext += [f"int ir_take_ptr_{t}(const {t} *p);", f"int ir_call_take_ptr_{t}(void);"]
    cases_a.append((f"take_ptr_{t}", f"{t} v = pat_{t}(); return ir_take_ptr_{t}(&v);"))
ext += ["void ir_layout(uint64_t *out);"]
H = [header] + CC + cc_layout + [""] + ext + [""]
H.append("int main(int argc, char **argv) {")
H.append('  if (argc == 2 && !strcmp(argv[1], "--list")) {')
for name, dirs in cases:
    H.append(f'    puts("{name} {dirs}");')
H.append('    puts("layout A");')
H.append("    return 0;")
H.append("  }")
H.append('  if (argc != 3) return 2;')
H.append("  char dir = argv[2][0];")
H.append('  if (!strcmp(argv[1], "layout")) {')
H.append(f"    uint64_t got[{2 * len(LAYOUT)}], want[{2 * len(LAYOUT)}];")
H.append("    ir_layout(got); c_layout_expect(want);")
H.append("    int bad = 0;")
for i, n in enumerate(names):
    H.append(f'    printf("  {n}: LLVM size %llu payload offset %llu (rule: %llu, %llu)\\n", (unsigned long long)got[{2*i}], (unsigned long long)got[{2*i+1}], (unsigned long long)want[{2*i}], (unsigned long long)want[{2*i+1}]);')
    H.append(f"    if (got[{2*i}] != want[{2*i}] || got[{2*i+1}] != want[{2*i+1}]) bad = 1;")
H.append("    return bad;")
H.append("  }")
for name, body in cases_a:
    H.append(f'  if (!strcmp(argv[1], "{name}") && dir == \'A\') {{ {body.replace("return", "return !(").rstrip(";")}); }}')
for name, dirs in cases:
    if "B" in dirs:
        H.append(f'  if (!strcmp(argv[1], "{name}") && dir == \'B\') return !ir_call_{name}();')
H.append("  return 2;")
H.append("}")
with open("harness.c", "w") as f:
    f.write("\n".join(H) + "\n")

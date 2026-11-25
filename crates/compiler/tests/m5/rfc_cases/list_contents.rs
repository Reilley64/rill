//! RFC 0007 O9 (ri-ema, rule R1): a `&mut List<&R>` destination holds the
//! element contents `&R`, like a `&mut &R` destination holds `&R`. The O9 row
//! in `rfc_cases.rs` pins the RFC's own module; these tests pin the forms
//! around it, the old caps (metering rule 11), the siblings that keep their
//! outcomes (rule 1) and the known conservative rejection.
use super::o12::{Case, check_cases};
use super::record_contents::{checked, incomplete, rejected};
use super::*;
use compiler::analysis::ownership::{Place, RetainedPlace};
use compiler::analysis::typed::ResultUseBlocker;

const CONSERVATIVE: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { }
function local(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
}
"####;

const DIRECT: &str = r####"
type User = { id: i32 };
function retain(dst: &mut List<&User>, source: &User): void { }
function forwarded(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
function local(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
}
function keep(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let view = &user;
}
function twice(dst: &mut List<&User>, a: &User, b: &User): void {
  retain(dst, a);
  retain(dst, b);
}
"####;

const FAMILY_FORMS: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function twice(dst: &mut List<&User>, a: &User, b: &User): void {
  retain(dst, a);
  retain(dst, b);
}
function grouped(dst: &mut List<&User>, source: &User): void { retain((dst), source); }
function looped(dst: &mut List<&User>, source: &User): void {
  while (true) { retain(dst, source); }
}
function keep(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let view = &user;
}
"####;

const PAIR: &str = r####"
type User = { id: i32 };
function retain<D, S, T>(dst: D, a: S, b: T): void;
implement retain(dst: &mut List<&User>, a: &User, b: &User): void { while (true) {} }
function both(dst: &mut List<&User>, a: &User, b: &User): void { retain(dst, a, b); }
"####;

const ALONE: &str = r####"
type User = { id: i32 };
function retain<D>(dst: D): void;
implement retain(dst: &mut List<&User>): void { while (true) {} }
function forwarded(dst: &mut List<&User>): void { retain(dst); }
"####;

const ESCAPES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function aliased(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let alias = dst;
  retain(alias, &user);
}
function viaView(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let view = &user;
  retain(dst, view);
}
function mixed(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, a);
  retain(dst, &user);
}
function branch(dst: &mut List<&User>, flag: bool): void {
  let user = User { id: 1 };
  if (flag) { retain(dst, &user); }
}
function dead(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  if (false) { retain(dst, &user); }
}
function loopLocal(dst: &mut List<&User>): void {
  while (true) {
    let user = User { id: 1 };
    retain(dst, &user);
  }
}
function viaCall(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  forward(dst, &user);
}
function forward(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
"####;

const AFTER_RETAIN: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function reuse(dst: &mut List<&User>, source: &User): i32 {
  retain(dst, source);
  return source.id;
}
function reuseDst(dst: &mut List<&User>, source: &User): void {
  retain(dst, source);
  retain(dst, source);
}
"####;

const NAMED: &str = r####"
type Named = { name: string; id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Named>, source: &Named): void { while (true) {} }
function forwarded(dst: &mut List<&Named>, source: &Named): void { retain(dst, source); }
"####;

const NESTED: &str = r####"
type Inner = { id: i32 };
type Outer = { inner: Inner; id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Outer>, source: &Outer): void { while (true) {} }
function forwarded(dst: &mut List<&Outer>, source: &Outer): void { retain(dst, source); }
"####;

const ENUM: &str = r####"
enum Kind { A, B }
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Kind>, source: &Kind): void { while (true) {} }
function forwarded(dst: &mut List<&Kind>, source: &Kind): void { retain(dst, source); }
"####;

const GENERIC: &str = r####"
type Box<T> = { value: T };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Box<i32>>, source: &Box<i32>): void { while (true) {} }
function forwarded(dst: &mut List<&Box<i32>>, source: &Box<i32>): void { retain(dst, source); }
"####;

const OWNED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<User>, source: &User): void { while (true) {} }
function forwarded(dst: &mut List<User>, source: &User): void { retain(dst, source); }
"####;

const EXCLUSIVE: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&mut User>, source: &mut User): void { while (true) {} }
function forwarded(dst: &mut List<&mut User>, source: &mut User): void { retain(dst, source); }
"####;

const SHARED_DESTINATION: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &List<&User>, source: &User): void { while (true) {} }
function forwarded(dst: &List<&User>, source: &User): void { retain(dst, source); }
"####;

const FLAG: &str = r####"
type User = { id: i32 };
function retain<D, S, F>(dst: D, source: S, flag: F): void;
implement retain(dst: &mut List<&User>, source: &User, flag: bool): void { while (true) {} }
function forwarded(dst: &mut List<&User>, source: &User, flag: bool): void { retain(dst, source, flag); }
"####;

const LOCAL_DESTINATION: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function fresh(source: &User): void {
  let mut users: List<&User> = [source];
  retain(&mut users, source);
}
"####;

const DIRECT_CALL: &str = r####"
type User = { id: i32 };
function retain(dst: &mut List<&User>, source: &User): void { }
function forwarded(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
"####;

const TWO_CALLS: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function twice(dst: &mut List<&User>, a: &User, b: &User): void {
  retain(dst, a);
  retain(dst, b);
}
"####;

const LOOPED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function looped(dst: &mut List<&User>, source: &User): void {
  while (true) { retain(dst, source); }
}
"####;

const AFTER_READ: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function reuse(dst: &mut List<&User>, source: &User): i32 {
  retain(dst, source);
  return source.id;
}
"####;

const LOOP_LOCAL: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function looped(dst: &mut List<&User>, source: &User): void {
  while (true) { retain(dst, source); }
}
function loopLocal(dst: &mut List<&User>): void {
  while (true) {
    let user = User { id: 1 };
    retain(dst, &user);
  }
}
function loopLocalReturn(dst: &mut List<&User>): void {
  while (true) {
    let user = User { id: 1 };
    retain(dst, &user);
    return;
  }
}
"####;

const OUTLIVES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function writeAfter(dst: &mut List<&User>): void {
  let mut user = User { id: 1 };
  retain(dst, &user);
  user.id = 2;
}
function readAfter(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
  let n = user.id;
}
function writeBefore(dst: &mut List<&User>): void {
  let mut user = User { id: 1 };
  user.id = 2;
  retain(dst, &user);
}
function viewReadThenWrite(): void {
  let mut user = User { id: 1 };
  let view = &user;
  let n = view.id;
  user.id = 2;
}
"####;

const EXCLUSIVE_SOURCE: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function viaReborrow(dst: &mut List<&User>, source: &mut User): void {
  retain(dst, &*source);
  source.id = 2;
}
"####;

const EXCLUSIVE_PASSED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function passed(dst: &mut List<&User>, source: &mut User): void {
  retain(dst, source);
  source.id = 2;
}
"####;

const GROUPED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function grouped(dst: &mut List<&User>, source: &User): void { retain((dst), source); }
"####;

const OLD_CONTENTS: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function forward(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
function reversed(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user);
  retain(dst, a);
}
function reversedViaCall(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user);
  forward(dst, a);
}
"####;

const ORDER: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function paramThenLocal(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, a);
  retain(dst, &user);
}
function localThenParam(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user);
  retain(dst, a);
}
"####;

const TWO_LOCALS: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function firstThenSecond(dst: &mut List<&User>): void {
  let one = User { id: 1 };
  let two = User { id: 2 };
  retain(dst, &one);
  retain(dst, &two);
}
function secondThenFirst(dst: &mut List<&User>): void {
  let one = User { id: 1 };
  let two = User { id: 2 };
  retain(dst, &two);
  retain(dst, &one);
}
function twoThenParam(dst: &mut List<&User>, a: &User): void {
  let one = User { id: 1 };
  let two = User { id: 2 };
  retain(dst, &two);
  retain(dst, &one);
  retain(dst, a);
}
function loopLocalThenParam(dst: &mut List<&User>, a: &User, flag: bool): void {
  let user = User { id: 1 };
  while (flag) {
    retain(dst, &user);
    retain(dst, a);
  }
}
"####;

const VIEW_BRANCHES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function v23_viewBranches(dst: &mut List<&User>, a: &User, flag: bool): void {
  let user = User { id: 1 };
  let view = &user;
  if (flag) { retain(dst, view); } else { retain(dst, view); }
  retain(dst, a);
}
function v37_viewsOfTwo(dst: &mut List<&User>, a: &User, flag: bool): void {
  let one = User { id: 1 };
  let two = User { id: 2 };
  let v = &two;
  let w = &one;
  if (flag) { retain(dst, v); } else { retain(dst, w); }
  retain(dst, v);
  retain(dst, w);
  retain(dst, a);
}
"####;

const SPLIT_STORES: &str = r####"
function set(slot: &mut &bool, input: &bool): void { *slot = input; }
function note(flag: bool): void {}
function c1_splitThenRepair(slot: &mut &bool, input: &bool, c: bool): void {
  let flag = true;
  let view = &flag;
  if (c) { set(slot, view); } else { set(slot, view); }
  set(slot, input);
  note(c);
  note(c);
  note(c);
  note(c);
  *slot = input;
}
function c2_split3ThenRepair(slot: &mut &bool, input: &bool, c: bool, d: bool): void {
  let flag = true;
  let view = &flag;
  if (c) { set(slot, view); } else { if (d) { set(slot, view); } else { set(slot, view); } }
  set(slot, input);
  note(c);
  note(c);
  note(c);
  note(c);
  *slot = input;
}
function c3_noSplitControl(slot: &mut &bool, input: &bool, c: bool): void {
  let flag = true;
  let view = &flag;
  if (c) { note(c); } else { note(c); }
  set(slot, view);
  set(slot, input);
  note(c);
  note(c);
  note(c);
  note(c);
  *slot = input;
}
"####;

const ASSIGNED_STORES: &str = r####"
type Point = { x: i32; y: i32 };
function set(slot: &mut &bool, input: &bool): void { *slot = input; }
function remember(slot: &mut &Point, source: &Point): void { *slot = source; }
function o3_assignLocalThenCall(slot: &mut &bool, input: &bool): void {
  let flag = true;
  *slot = &flag;
  set(slot, input);
}
function r4_assignLocalThenCall(slot: &mut &Point, a: &Point): void {
  let p = Point { x: 1, y: 2 };
  *slot = &p;
  remember(slot, a);
}
function o8_branchAssign(slot: &mut &bool, input: &bool, c: bool): void {
  let flag = true;
  if (c) { *slot = &flag; }
  set(slot, input);
}
function o5_callLocalAssignLocalCall(slot: &mut &bool, input: &bool): void {
  let flag = true;
  let other = false;
  set(slot, &flag);
  *slot = &other;
  set(slot, input);
}
function o7_assignLocalCallLocalCall(slot: &mut &bool, input: &bool): void {
  let flag = true;
  let other = false;
  *slot = &flag;
  set(slot, &other);
  set(slot, input);
}
function oa_assignLocalOnly(slot: &mut &bool, input: &bool): void {
  let flag = true;
  *slot = &flag;
}
"####;

const STORE_PREMISES: &str = r####"
function set(slot: &mut &bool, input: &bool): void { *slot = input; }
function keep(slot: &mut &bool, input: &bool): void { *slot = input; }
function repaired(slot: &mut &bool, input: &bool): void {
  let flag = true;
  let view = &flag;
  set(slot, view);
  *slot = input;
  keep(slot, view);
}
function joinedSetFirst(slot: &mut &bool, input: &bool, c: bool): void {
  let flag = true;
  let view = &flag;
  if (c) { set(slot, view); } else { keep(slot, view); }
  set(slot, input);
}
function joinedKeepFirst(slot: &mut &bool, input: &bool, c: bool): void {
  let flag = true;
  let view = &flag;
  if (c) { keep(slot, view); } else { set(slot, view); }
  set(slot, input);
}
"####;

const OWNED_SOURCES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function byValue(dst: &mut List<&User>, owned: User): void { retain(dst, &owned); }
function hop(dst: &mut List<&User>, owned: User): void {
  let view = &owned;
  retain(dst, view);
}
function oneBranch(dst: &mut List<&User>, owned: User, flag: bool): void {
  if (flag) { retain(dst, &owned); }
}
function loopOwned(dst: &mut List<&User>, owned: User, flag: bool): void {
  while (flag) { retain(dst, &owned); }
}
function loopBreak(dst: &mut List<&User>, owned: User): void {
  while (true) { retain(dst, &owned); break; }
}
"####;

const TEMPORARY_SOURCES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function make(): User { return User { id: 1 }; }
function call(dst: &mut List<&User>): void { retain(dst, &make()); }
function literal(dst: &mut List<&User>): void { retain(dst, &User { id: 1 }); }
"####;

const PAIR_LOCAL: &str = r####"
type User = { id: i32 };
function retain<D, S, T>(dst: D, a: S, b: T): void;
implement retain(dst: &mut List<&User>, a: &User, b: &User): void { while (true) {} }
function localSecond(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, a, &user);
}
function localFirst(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user, a);
}
"####;

const LONE_LOCAL: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function touch<D>(dst: D): void;
implement touch(dst: &mut List<&User>): void { while (true) {} }
function loneAfterLocal(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
  touch(dst);
}
function loneBeforeLocal(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  touch(dst);
  retain(dst, &user);
}
"####;

const NAMED_BORROW: &str = r####"type T = { name: string; id: i32 };
function touch(dst: &mut List<&T>): void { }
function touchTwo(dst: &mut List<&T>, source: &T): void { }
function touchF<D, S>(dst: D, source: S): void;
implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }
function touchOne<D>(dst: D): void;
implement touchOne(dst: &mut List<&T>): void { while (true) {} }
function f(): void { let a = T { name: "n", id: 1 }; let mut users: List<&T> = [&a]; let r = &mut users; }
"####;

const NAMED_LONE: &str = r####"type T = { name: string; id: i32 };
function touch(dst: &mut List<&T>): void { }
function touchTwo(dst: &mut List<&T>, source: &T): void { }
function touchF<D, S>(dst: D, source: S): void;
implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }
function touchOne<D>(dst: D): void;
implement touchOne(dst: &mut List<&T>): void { while (true) {} }
function f(dst: &mut List<&T>): void { touchOne(dst); }
"####;

const NAMED_FORWARDED: &str = r####"type T = { name: string; id: i32 };
function touch(dst: &mut List<&T>): void { }
function touchTwo(dst: &mut List<&T>, source: &T): void { }
function touchF<D, S>(dst: D, source: S): void;
implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }
function touchOne<D>(dst: D): void;
implement touchOne(dst: &mut List<&T>): void { while (true) {} }
function f(dst: &mut List<&T>, s: &T): void { touchF(dst, s); }
"####;

const NAMED_FORWARDED_PIPE: &str = r####"type T = { name: string; id: i32 };
function touch(dst: &mut List<&T>): void { }
function touchTwo(dst: &mut List<&T>, source: &T): void { }
function touchF<D, S>(dst: D, source: S): void;
implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }
function touchOne<D>(dst: D): void;
implement touchOne(dst: &mut List<&T>): void { while (true) {} }
function f(dst: &mut List<&T>, s: &T): void { dst |> touchF(s); }
"####;

const NAMED_DIRECT_EMPTY: &str = r####"type T = { name: string; id: i32 };
function touch(dst: &mut List<&T>): void { }
function touchTwo(dst: &mut List<&T>, source: &T): void { }
function touchF<D, S>(dst: D, source: S): void;
implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }
function touchOne<D>(dst: D): void;
implement touchOne(dst: &mut List<&T>): void { while (true) {} }
function f(dst: &mut List<&T>): void { touch(dst); }
"####;

const NAMED_DIRECT_PAIR: &str = r####"type T = { name: string; id: i32 };
function touch(dst: &mut List<&T>): void { }
function touchTwo(dst: &mut List<&T>, source: &T): void { }
function touchF<D, S>(dst: D, source: S): void;
implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }
function touchOne<D>(dst: D): void;
implement touchOne(dst: &mut List<&T>): void { while (true) {} }
function f(dst: &mut List<&T>, s: &T): void { touchTwo(dst, s); }
"####;

const NAMED_PIPED_PAIR: &str = r####"type T = { name: string; id: i32 };
function touch(dst: &mut List<&T>): void { }
function touchTwo(dst: &mut List<&T>, source: &T): void { }
function touchF<D, S>(dst: D, source: S): void;
implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }
function touchOne<D>(dst: D): void;
implement touchOne(dst: &mut List<&T>): void { while (true) {} }
function f(dst: &mut List<&T>, s: &T): void { dst |> touchTwo(s); }
"####;

/// The nested-record and enum forms of the seven `NAMED_*` modules above, so a
/// `&mut List<&R>` parameter of each unsupported record shape is measured at
/// the same positions (`ri-px1.11.11`, C-6).
macro_rules! list_parameter_text {
    ($header:literal, $f:literal) => {
        concat!(
            $header,
            "\nfunction touch(dst: &mut List<&T>): void { }\n",
            "function touchTwo(dst: &mut List<&T>, source: &T): void { }\n",
            "function touchF<D, S>(dst: D, source: S): void;\n",
            "implement touchF(dst: &mut List<&T>, source: &T): void { while (true) {} }\n",
            "function touchOne<D>(dst: D): void;\n",
            "implement touchOne(dst: &mut List<&T>): void { while (true) {} }\n",
            $f,
            "\n"
        )
    };
}

const NESTED_BORROW: &str = list_parameter_text!(
    "type Inner = { id: i32 };\ntype T = { inner: Inner; id: i32 };",
    "function f(): void { let a = T { inner: Inner { id: 1 }, id: 2 }; let mut users: List<&T> = [&a]; let r = &mut users; }"
);
const NESTED_LONE: &str = list_parameter_text!(
    "type Inner = { id: i32 };\ntype T = { inner: Inner; id: i32 };",
    "function f(dst: &mut List<&T>): void { touchOne(dst); }"
);
const NESTED_FORWARDED: &str = list_parameter_text!(
    "type Inner = { id: i32 };\ntype T = { inner: Inner; id: i32 };",
    "function f(dst: &mut List<&T>, s: &T): void { touchF(dst, s); }"
);
const NESTED_FORWARDED_PIPE: &str = list_parameter_text!(
    "type Inner = { id: i32 };\ntype T = { inner: Inner; id: i32 };",
    "function f(dst: &mut List<&T>, s: &T): void { dst |> touchF(s); }"
);
const NESTED_DIRECT_EMPTY: &str = list_parameter_text!(
    "type Inner = { id: i32 };\ntype T = { inner: Inner; id: i32 };",
    "function f(dst: &mut List<&T>): void { touch(dst); }"
);
const NESTED_DIRECT_PAIR: &str = list_parameter_text!(
    "type Inner = { id: i32 };\ntype T = { inner: Inner; id: i32 };",
    "function f(dst: &mut List<&T>, s: &T): void { touchTwo(dst, s); }"
);
const NESTED_PIPED_PAIR: &str = list_parameter_text!(
    "type Inner = { id: i32 };\ntype T = { inner: Inner; id: i32 };",
    "function f(dst: &mut List<&T>, s: &T): void { dst |> touchTwo(s); }"
);
const ENUM_BORROW: &str = list_parameter_text!(
    "enum T { A, B }",
    "function f(): void { let a = T.A; let mut users: List<&T> = [&a]; let r = &mut users; }"
);
const ENUM_LONE: &str = list_parameter_text!(
    "enum T { A, B }",
    "function f(dst: &mut List<&T>): void { touchOne(dst); }"
);
const ENUM_FORWARDED: &str = list_parameter_text!(
    "enum T { A, B }",
    "function f(dst: &mut List<&T>, s: &T): void { touchF(dst, s); }"
);
const ENUM_FORWARDED_PIPE: &str = list_parameter_text!(
    "enum T { A, B }",
    "function f(dst: &mut List<&T>, s: &T): void { dst |> touchF(s); }"
);
const ENUM_DIRECT_EMPTY: &str = list_parameter_text!(
    "enum T { A, B }",
    "function f(dst: &mut List<&T>): void { touch(dst); }"
);
const ENUM_DIRECT_PAIR: &str = list_parameter_text!(
    "enum T { A, B }",
    "function f(dst: &mut List<&T>, s: &T): void { touchTwo(dst, s); }"
);
const ENUM_PIPED_PAIR: &str = list_parameter_text!(
    "enum T { A, B }",
    "function f(dst: &mut List<&T>, s: &T): void { dst |> touchTwo(s); }"
);

const R0202_LOCAL: (&str, &str, &str) = ("R0202", "retain(dst, &user)", "&user");

/// R1's known conservative rejection (the chief's condition, ri-ema). The
/// implementation stores nothing, so the program is valid: `user` ends with
/// the call's caller and `dst` never holds it. Rule R1 reads only the
/// declared signature, assumes the `&User` input can flow into `dst`'s
/// elements and rejects the caller's body-local owner as an escape (R0202).
/// A later refinement (R2, an explicit storage contract) can turn this into
/// `Checked`; this test is its witness.
#[test]
fn r1_known_conservative_rejection_of_a_callee_that_stores_nothing() {
    installed(
        "rfc_cases::list_contents::r1_known_conservative_rejection_of_a_callee_that_stores_nothing",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: CONSERVATIVE,
                        typed_stats: (326828, 48201, 12),
                        graph: (GraphCompletion::SourceRejected, (189799, 4419, 6)),
                        roots: &[rejected(
                            "local",
                            R0202_LOCAL.0,
                            R0202_LOCAL.1,
                            R0202_LOCAL.2,
                            (3696, 317, 6),
                        )],
                    }
                }],
            );
            old_root_caps(
                &request,
                &[(
                    CONSERVATIVE,
                    &[("local", (2262, 103, 3))],
                    (188363, 4204, 5),
                )],
            );
        },
    );
}

/// The direct-function form of O9: the contract is the declared signature,
/// the destination is the caller's `&mut List<&User>` parameter. Forwarding a
/// parameter, twice, and a view that is never stored check; a body-local
/// owner is R0202.
#[test]
fn direct_list_destination_checks_parameters_and_rejects_locals() {
    installed(
        "rfc_cases::list_contents::direct_list_destination_checks_parameters_and_rejects_locals",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: DIRECT,
                        typed_stats: (330577, 48520, 12),
                        graph: (GraphCompletion::SourceRejected, (194154, 5367, 6)),
                        roots: &[
                            checked("retain", (2196, 149, 1)),
                            checked("forwarded", (4715, 386, 5)),
                            rejected(
                                "local",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (3011, 360, 6),
                            ),
                            checked("keep", (2760, 305, 4)),
                            checked("twice", (5284, 590, 5)),
                        ],
                    }
                }],
            );
            old_root_caps(
                &request,
                &[(
                    DIRECT,
                    &[
                        ("retain", (2066, 121, 1)),
                        ("forwarded", (1995, 86, 3)),
                        ("local", (2356, 149, 3)),
                        ("keep", (2584, 260, 4)),
                        ("twice", (2001, 90, 3)),
                    ],
                    (191296, 4541, 5),
                )],
            );
        },
    );
}

/// Family calls with a list destination: two calls, a call in a loop, a pair
/// of sources, a lone destination and a source read after the call check. A
/// grouped destination keeps its parent blocker. Old caps give `ResourceLimit`.
#[test]
fn family_list_destination_forms() {
    installed(
        "rfc_cases::list_contents::family_list_destination_forms",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: FAMILY_FORMS,
                            typed_stats: (334244, 49369, 12),
                            graph: (GraphCompletion::Incomplete, (196629, 5148, 7)),
                            roots: &[
                                checked("twice", (4763, 488, 5)),
                                incomplete("grouped", "retain((dst), source)", (2069, 90, 3)),
                                checked("looped", (3562, 363, 7)),
                                checked("keep", (2769, 309, 4)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: PAIR,
                            typed_stats: (326630, 48231, 12),
                            graph: (GraphCompletion::Incomplete, (190024, 4414, 5)),
                            roots: &[checked("both", (3490, 312, 5))],
                        }
                    },
                    const {
                        Case {
                            text: ALONE,
                            typed_stats: (325747, 47893, 12),
                            graph: (GraphCompletion::Incomplete, (188935, 4258, 5)),
                            roots: &[checked("forwarded", (3133, 156, 5))],
                        }
                    },
                    const {
                        Case {
                            text: AFTER_RETAIN,
                            typed_stats: (329503, 48653, 12),
                            graph: (GraphCompletion::Incomplete, (193688, 4807, 5)),
                            roots: &[
                                checked("reuse", (3508, 322, 5)),
                                checked("reuseDst", (4630, 423, 5)),
                            ],
                        }
                    },
                ],
            );
            old_root_caps(
                &request,
                &[
                    (
                        FAMILY_FORMS,
                        &[
                            ("twice", (2000, 91, 3)),
                            ("grouped", (1995, 87, 3)),
                            ("looped", (1998, 88, 5)),
                            ("keep", (2593, 264, 4)),
                        ],
                        (192037, 4424, 5),
                    ),
                    (PAIR, &[("both", (1921, 51, 3))], (188446, 4152, 5)),
                    (ALONE, &[("forwarded", (1882, 31, 3))], (187679, 4132, 5)),
                ],
            );
        },
    );
}

/// Rule R1 never accepts an invalid program: a body-local owner stored
/// through an alias, a local view, a branch, a later call or a callee that
/// forwards it is R0202 at the call (a loop body that ends first is R0203);
/// the same store in dead code checks and rejects nothing (rule 6), and
/// forwarding a parameter checks.
#[test]
fn local_sources_escape_through_every_route_to_the_list() {
    installed(
        "rfc_cases::list_contents::local_sources_escape_through_every_route_to_the_list",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: ESCAPES,
                        typed_stats: (347203, 51623, 12),
                        graph: (GraphCompletion::SourceRejected, (210865, 7021, 8)),
                        roots: &[
                            rejected(
                                "aliased",
                                "R0202",
                                "retain(alias, &user)",
                                "&user",
                                (4042, 485, 6),
                            ),
                            rejected(
                                "viaView",
                                "R0202",
                                "retain(dst, view)",
                                "&user",
                                (4034, 483, 5),
                            ),
                            rejected(
                                "mixed",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (5290, 652, 6),
                            ),
                            rejected(
                                "branch",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (4269, 606, 8),
                            ),
                            checked("dead", (2868, 371, 4)),
                            rejected(
                                "loopLocal",
                                "R0203",
                                r####"{\n    let user = User { id: 1 };\n    retain(dst, &user);\n  }"####,
                                "&user",
                                (4122, 525, 8),
                            ),
                            rejected(
                                "viaCall",
                                "R0202",
                                "forward(dst, &user)",
                                "&user",
                                (3166, 437, 6),
                            ),
                            checked("forward", (3540, 348, 5)),
                        ],
                    }
                }],
            );
        },
    );
}

#[test]
fn list_destination_siblings_keep_their_outcomes_and_blockers() {
    installed(
        "rfc_cases::list_contents::list_destination_siblings_keep_their_outcomes_and_blockers",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: NAMED,
                            typed_stats: (326422, 48080, 12),
                            graph: (GraphCompletion::Incomplete, (188274, 4146, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (2052, 45, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: NESTED,
                            typed_stats: (327424, 48126, 12),
                            graph: (GraphCompletion::Incomplete, (188340, 4147, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (2054, 46, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: ENUM,
                            typed_stats: (326074, 48119, 12),
                            graph: (GraphCompletion::Incomplete, (188002, 4141, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1900, 40, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: FLAG,
                            typed_stats: (326920, 48195, 12),
                            graph: (GraphCompletion::Incomplete, (188396, 4153, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source, flag)",
                                (1991, 52, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: GENERIC,
                            typed_stats: (326941, 48156, 12),
                            graph: (GraphCompletion::Incomplete, (188312, 4146, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1910, 45, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: OWNED,
                            typed_stats: (326132, 48033, 12),
                            graph: (GraphCompletion::Incomplete, (187939, 4140, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1898, 39, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: EXCLUSIVE,
                            typed_stats: (326185, 48062, 12),
                            graph: (GraphCompletion::Incomplete, (188063, 4142, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1902, 41, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: SHARED_DESTINATION,
                            typed_stats: (326185, 48062, 12),
                            graph: (GraphCompletion::Incomplete, (188063, 4142, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1902, 41, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: GROUPED,
                            typed_stats: (327168, 48238, 12),
                            graph: (GraphCompletion::Incomplete, (188200, 4146, 5)),
                            roots: &[incomplete(
                                "grouped",
                                "retain((dst), source)",
                                (1979, 45, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: LOCAL_DESTINATION,
                            typed_stats: (326881, 48212, 12),
                            graph: (GraphCompletion::Incomplete, (188230, 4139, 5)),
                            roots: &[incomplete("fresh", "users", (1889, 38, 1))],
                        }
                    },
                ],
            );
            old_root_caps(
                &request,
                &[
                    (NAMED, &[("forwarded", (1904, 42, 3))], (188126, 4143, 5)),
                    (NESTED, &[("forwarded", (1906, 43, 3))], (188192, 4144, 5)),
                    (FLAG, &[("forwarded", (1917, 49, 3))], (188322, 4150, 5)),
                    (GROUPED, &[("grouped", (1905, 42, 3))], (188126, 4143, 5)),
                ],
            );
        },
    );
}

#[test]
fn list_parameter_siblings_pay_their_lookup_and_keep_their_outcomes() {
    installed(
        "rfc_cases::list_contents::list_parameter_siblings_pay_their_lookup_and_keep_their_outcomes",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: NAMED_BORROW,
                            typed_stats: (330963, 48419, 12),
                            graph: (GraphCompletion::Incomplete, (192013, 4322, 5)),
                            roots: &[
                                checked("touch", (2071, 91, 1)),
                                checked("touchTwo", (2114, 109, 1)),
                                incomplete("f", "users", (2303, 121, 1)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NAMED_LONE,
                            typed_stats: (331005, 48359, 12),
                            graph: (GraphCompletion::Incomplete, (191375, 4263, 5)),
                            roots: &[
                                checked("touch", (2057, 84, 1)),
                                checked("touchTwo", (2100, 102, 1)),
                                incomplete("f", "touchOne(dst)", (2087, 62, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NAMED_FORWARDED,
                            typed_stats: (331251, 48464, 12),
                            graph: (GraphCompletion::Incomplete, (191571, 4271, 5)),
                            roots: &[
                                checked("touch", (2063, 87, 1)),
                                checked("touchTwo", (2106, 105, 1)),
                                incomplete("f", "touchF(dst, s)", (2102, 70, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NAMED_FORWARDED_PIPE,
                            typed_stats: (331261, 48468, 12),
                            graph: (GraphCompletion::Incomplete, (191497, 4268, 5)),
                            roots: &[
                                checked("touch", (2063, 87, 1)),
                                checked("touchTwo", (2106, 105, 1)),
                                incomplete("f", "dst |> touchF(s)", (2028, 67, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NAMED_DIRECT_EMPTY,
                            typed_stats: (330572, 48270, 12),
                            graph: (GraphCompletion::Incomplete, (191442, 4264, 5)),
                            roots: &[
                                checked("touch", (2059, 85, 1)),
                                checked("touchTwo", (2102, 103, 1)),
                                incomplete("f", "dst", (2094, 63, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NAMED_DIRECT_PAIR,
                            typed_stats: (330779, 48324, 12),
                            graph: (GraphCompletion::Incomplete, (191638, 4272, 5)),
                            roots: &[
                                checked("touch", (2065, 88, 1)),
                                checked("touchTwo", (2108, 106, 1)),
                                incomplete("f", "dst", (2109, 71, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NAMED_PIPED_PAIR,
                            typed_stats: (330789, 48328, 12),
                            graph: (GraphCompletion::Incomplete, (191638, 4269, 5)),
                            roots: &[
                                checked("touch", (2065, 88, 1)),
                                checked("touchTwo", (2108, 106, 1)),
                                incomplete("f", "dst |> touchTwo(s)", (2109, 68, 3)),
                            ],
                        }
                    },
                ],
            );
            old_root_caps(
                &request,
                &[
                    (
                        NAMED_BORROW,
                        &[("touch", (1999, 91, 1)), ("touchTwo", (2042, 109, 1))],
                        (191869, 4322, 5),
                    ),
                    (
                        NAMED_LONE,
                        &[
                            ("touch", (1985, 84, 1)),
                            ("touchTwo", (2028, 102, 1)),
                            ("f", (1940, 60, 3)),
                        ],
                        (191084, 4261, 5),
                    ),
                    (
                        NAMED_FORWARDED,
                        &[
                            ("touch", (1991, 87, 1)),
                            ("touchTwo", (2034, 105, 1)),
                            ("f", (1954, 67, 3)),
                        ],
                        (191279, 4268, 5),
                    ),
                    (
                        NAMED_FORWARDED_PIPE,
                        &[
                            ("touch", (1991, 87, 1)),
                            ("touchTwo", (2034, 105, 1)),
                            ("f", (1956, 67, 3)),
                        ],
                        (191281, 4268, 5),
                    ),
                    (
                        NAMED_DIRECT_EMPTY,
                        &[
                            ("touch", (1987, 85, 1)),
                            ("touchTwo", (2030, 103, 1)),
                            ("f", (1950, 63, 3)),
                        ],
                        (191154, 4264, 5),
                    ),
                    (
                        NAMED_DIRECT_PAIR,
                        &[
                            ("touch", (1993, 88, 1)),
                            ("touchTwo", (2036, 106, 1)),
                            ("f", (1965, 71, 3)),
                        ],
                        (191350, 4272, 5),
                    ),
                    (
                        NAMED_PIPED_PAIR,
                        &[
                            ("touch", (1993, 88, 1)),
                            ("touchTwo", (2036, 106, 1)),
                            ("f", (1965, 68, 3)),
                        ],
                        (191350, 4269, 5),
                    ),
                ],
            );
        },
    );
}

#[test]
fn list_parameter_siblings_of_nested_records_and_enums_pay_their_lookup() {
    installed(
        "rfc_cases::list_contents::list_parameter_siblings_of_nested_records_and_enums_pay_their_lookup",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: NESTED_BORROW,
                            typed_stats: (332080, 48509, 12),
                            graph: (GraphCompletion::Incomplete, (192502, 4382, 5)),
                            roots: &[
                                checked("touch", (2075, 93, 1)),
                                checked("touchTwo", (2118, 111, 1)),
                                incomplete("f", "users", (2668, 181, 1)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NESTED_LONE,
                            typed_stats: (332023, 48405, 12),
                            graph: (GraphCompletion::Incomplete, (191441, 4264, 5)),
                            roots: &[
                                checked("touch", (2059, 85, 1)),
                                checked("touchTwo", (2102, 103, 1)),
                                incomplete("f", "touchOne(dst)", (2089, 63, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NESTED_FORWARDED,
                            typed_stats: (332268, 48510, 12),
                            graph: (GraphCompletion::Incomplete, (191637, 4272, 5)),
                            roots: &[
                                checked("touch", (2065, 88, 1)),
                                checked("touchTwo", (2108, 106, 1)),
                                incomplete("f", "touchF(dst, s)", (2104, 71, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NESTED_FORWARDED_PIPE,
                            typed_stats: (332278, 48514, 12),
                            graph: (GraphCompletion::Incomplete, (191563, 4269, 5)),
                            roots: &[
                                checked("touch", (2065, 88, 1)),
                                checked("touchTwo", (2108, 106, 1)),
                                incomplete("f", "dst |> touchF(s)", (2030, 68, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NESTED_DIRECT_EMPTY,
                            typed_stats: (331590, 48316, 12),
                            graph: (GraphCompletion::Incomplete, (191508, 4265, 5)),
                            roots: &[
                                checked("touch", (2061, 86, 1)),
                                checked("touchTwo", (2104, 104, 1)),
                                incomplete("f", "dst", (2096, 64, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NESTED_DIRECT_PAIR,
                            typed_stats: (331796, 48370, 12),
                            graph: (GraphCompletion::Incomplete, (191704, 4273, 5)),
                            roots: &[
                                checked("touch", (2067, 89, 1)),
                                checked("touchTwo", (2110, 107, 1)),
                                incomplete("f", "dst", (2111, 72, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NESTED_PIPED_PAIR,
                            typed_stats: (331806, 48374, 12),
                            graph: (GraphCompletion::Incomplete, (191704, 4270, 5)),
                            roots: &[
                                checked("touch", (2067, 89, 1)),
                                checked("touchTwo", (2110, 107, 1)),
                                incomplete("f", "dst |> touchTwo(s)", (2111, 69, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: ENUM_BORROW,
                            typed_stats: (330391, 48360, 12),
                            graph: (GraphCompletion::Incomplete, (191255, 4256, 5)),
                            roots: &[
                                checked("touch", (1991, 87, 1)),
                                checked("touchTwo", (2034, 105, 1)),
                                incomplete("f", "a", (1929, 55, 1)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: ENUM_LONE,
                            typed_stats: (330588, 48358, 12),
                            graph: (GraphCompletion::Incomplete, (190960, 4259, 5)),
                            roots: &[
                                checked("touch", (1981, 82, 1)),
                                checked("touchTwo", (2024, 100, 1)),
                                incomplete("f", "touchOne(dst)", (1936, 58, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: ENUM_FORWARDED,
                            typed_stats: (330903, 48503, 12),
                            graph: (GraphCompletion::Incomplete, (191155, 4266, 5)),
                            roots: &[
                                checked("touch", (1987, 85, 1)),
                                checked("touchTwo", (2030, 103, 1)),
                                incomplete("f", "touchF(dst, s)", (1950, 65, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: ENUM_FORWARDED_PIPE,
                            typed_stats: (330913, 48507, 12),
                            graph: (GraphCompletion::Incomplete, (191157, 4266, 5)),
                            roots: &[
                                checked("touch", (1987, 85, 1)),
                                checked("touchTwo", (2030, 103, 1)),
                                incomplete("f", "dst |> touchF(s)", (1952, 65, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: ENUM_DIRECT_EMPTY,
                            typed_stats: (330155, 48269, 12),
                            graph: (GraphCompletion::Incomplete, (191030, 4262, 5)),
                            roots: &[
                                checked("touch", (1983, 83, 1)),
                                checked("touchTwo", (2026, 101, 1)),
                                incomplete("f", "dst", (1946, 61, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: ENUM_DIRECT_PAIR,
                            typed_stats: (330359, 48323, 12),
                            graph: (GraphCompletion::Incomplete, (191226, 4270, 5)),
                            roots: &[
                                checked("touch", (1989, 86, 1)),
                                checked("touchTwo", (2032, 104, 1)),
                                incomplete("f", "dst", (1961, 69, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: ENUM_PIPED_PAIR,
                            typed_stats: (330369, 48327, 12),
                            graph: (GraphCompletion::Incomplete, (191226, 4267, 5)),
                            roots: &[
                                checked("touch", (1989, 86, 1)),
                                checked("touchTwo", (2032, 104, 1)),
                                incomplete("f", "dst |> touchTwo(s)", (1961, 66, 3)),
                            ],
                        }
                    },
                ],
            );
            old_root_caps(
                &request,
                &[
                    (
                        NESTED_BORROW,
                        &[("touch", (2003, 93, 1)), ("touchTwo", (2046, 111, 1))],
                        (192358, 4382, 5),
                    ),
                    (
                        NESTED_LONE,
                        &[
                            ("touch", (1987, 85, 1)),
                            ("touchTwo", (2030, 103, 1)),
                            ("f", (1942, 61, 3)),
                        ],
                        (191150, 4262, 5),
                    ),
                    (
                        NESTED_FORWARDED,
                        &[
                            ("touch", (1993, 88, 1)),
                            ("touchTwo", (2036, 106, 1)),
                            ("f", (1956, 68, 3)),
                        ],
                        (191345, 4269, 5),
                    ),
                    (
                        NESTED_FORWARDED_PIPE,
                        &[
                            ("touch", (1993, 88, 1)),
                            ("touchTwo", (2036, 106, 1)),
                            ("f", (1958, 68, 3)),
                        ],
                        (191347, 4269, 5),
                    ),
                    (
                        NESTED_DIRECT_EMPTY,
                        &[
                            ("touch", (1989, 86, 1)),
                            ("touchTwo", (2032, 104, 1)),
                            ("f", (1952, 64, 3)),
                        ],
                        (191220, 4265, 5),
                    ),
                    (
                        NESTED_DIRECT_PAIR,
                        &[
                            ("touch", (1995, 89, 1)),
                            ("touchTwo", (2038, 107, 1)),
                            ("f", (1967, 72, 3)),
                        ],
                        (191416, 4273, 5),
                    ),
                    (
                        NESTED_PIPED_PAIR,
                        &[
                            ("touch", (1995, 89, 1)),
                            ("touchTwo", (2038, 107, 1)),
                            ("f", (1967, 69, 3)),
                        ],
                        (191416, 4270, 5),
                    ),
                ],
            );
        },
    );
}

/// Condition 1 (chief of staff, 2026-11-24 12:35): the direct-function form,
/// no family. `forwarded` passes its own `dst` and `source` to `retain`: a
/// mutable reborrow of `dst`, a copy of the source, the retention into the
/// destination's contents, then the parameters end. An empty implementation
/// body checks (the call reads the declared signature).
#[test]
fn r1_direct_function_retain_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_direct_function_retain_checks_with_its_sequence",
        const {
            Case {
                text: DIRECT_CALL,
                typed_stats: (324763, 47779, 12),
                graph: (GraphCompletion::Incomplete, (187650, 4387, 5)),
                roots: &[
                    checked("retain", (2102, 102, 1)),
                    checked("forwarded", (4527, 292, 5)),
                ],
            }
        },
        &[
            (
                "retain",
                &[
                    "0: goto 1",
                    "1: end source: &User; end dst: &mut List<&User>; goto 2",
                    "2: return",
                ],
            ),
            (
                "forwarded",
                &[
                    "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                    "1: copy source: &User -> source; goto 2",
                    "2: retain through dst: &mut List<&User> <- source; goto 3",
                    "3: goto 4",
                    "4: goto 5",
                    "5: end source: &User; end dst: &mut List<&User>; goto 6",
                    "6: return",
                ],
            ),
        ],
    );
}

/// Condition 1: two calls into one destination. Each call reborrows `dst`
/// and retains its own source; both sources reach the contents in call order.
#[test]
fn r1_two_calls_check_with_their_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_two_calls_check_with_their_sequence",
        const {
            Case {
                text: TWO_CALLS,
                typed_stats: (327109, 48308, 12),
                graph: (GraphCompletion::Incomplete, (191217, 4551, 5)),
                roots: &[checked("twice", (4683, 448, 5))],
            }
        },
        &[(
            "twice",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: copy a: &User -> a; goto 2",
                "2: retain through dst: &mut List<&User> <- a; goto 3",
                "3: goto 4",
                "4: reborrow mut dst: &mut List<&User> -> dst; goto 5",
                "5: copy b: &User -> b; goto 6",
                "6: retain through dst: &mut List<&User> <- b; goto 7",
                "7: goto 8",
                "8: goto 9",
                "9: end b: &User; end a: &User; end dst: &mut List<&User>; goto 10",
                "10: return",
            ],
        )],
    );
}

/// Condition 1: a call in a loop. The block graph holds the loop's backedge
/// (the last retain jumps to block 5) and no return: the loop never exits.
#[test]
fn r1_call_in_a_loop_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_call_in_a_loop_checks_with_its_sequence",
        const {
            Case {
                text: LOOPED,
                typed_stats: (326300, 48095, 12),
                graph: (GraphCompletion::Incomplete, (189820, 4423, 7)),
                roots: &[checked("looped", (3476, 320, 7))],
            }
        },
        &[(
            "looped",
            &[
                "0: goto 1",
                "1: goto 2",
                "2: reborrow mut dst: &mut List<&User> -> dst; goto 3",
                "3: copy source: &User -> source; goto 4",
                "4: retain through dst: &mut List<&User> <- source; goto 5",
                "5: goto 6",
                "6: goto 7",
                "7: goto 8",
                "8: goto 9",
                "9: reborrow mut dst: &mut List<&User> -> dst; goto 10",
                "10: copy source: &User -> source; goto 11",
                "11: retain through dst: &mut List<&User> <- source; goto 5",
            ],
        )],
    );
}

/// Condition 1: a pair of `&User` sources, both staged before the one
/// retention, in parameter order.
#[test]
fn r1_pair_of_user_sources_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_pair_of_user_sources_checks_with_its_sequence",
        const {
            Case {
                text: PAIR,
                typed_stats: (326630, 48231, 12),
                graph: (GraphCompletion::Incomplete, (190024, 4414, 5)),
                roots: &[checked("both", (3490, 312, 5))],
            }
        },
        &[(
            "both",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: copy a: &User -> a; goto 2",
                "2: copy b: &User -> b; goto 3",
                "3: retain through dst: &mut List<&User> <- a, b; goto 4",
                "4: goto 5",
                "5: goto 6",
                "6: end b: &User; end a: &User; end dst: &mut List<&User>; goto 7",
                "7: return",
            ],
        )],
    );
}

/// Condition 1: a lone destination. `retain(dst)` adds no source and ends
/// none ("the contents keep every source they already held").
#[test]
fn r1_lone_destination_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_lone_destination_checks_with_its_sequence",
        const {
            Case {
                text: ALONE,
                typed_stats: (325747, 47893, 12),
                graph: (GraphCompletion::Incomplete, (188935, 4258, 5)),
                roots: &[checked("forwarded", (3133, 156, 5))],
            }
        },
        &[(
            "forwarded",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: retain through dst: &mut List<&User> <- nothing; goto 2",
                "2: goto 3",
                "3: goto 4",
                "4: end dst: &mut List<&User>; goto 5",
                "5: return",
            ],
        )],
    );
}

/// Condition 1: the source is read after the call. The retention does not
/// end the caller's `source`: the read (`deref`, then `export`) follows it.
#[test]
fn r1_source_read_after_the_call_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_source_read_after_the_call_checks_with_its_sequence",
        const {
            Case {
                text: AFTER_READ,
                typed_stats: (326456, 48106, 12),
                graph: (GraphCompletion::Incomplete, (189825, 4408, 5)),
                roots: &[checked("reuse", (3476, 306, 5))],
            }
        },
        &[(
            "reuse",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: copy source: &User -> source; goto 2",
                "2: retain through dst: &mut List<&User> <- source; goto 3",
                "3: goto 4",
                "4: deref source: &User; goto 5",
                "5: export source.id; goto 6",
                "6: goto 7",
                "7: end source: &User; end dst: &mut List<&User>; goto 8",
                "8: return",
            ],
        )],
    );
}

/// Condition 1: `keep` (a `&mut List<&User>` parameter and no call) is pinned
/// by name at 2,760 / 305 / 4 in the module of the direct-function test
/// (`DIRECT`, whose `local` rejects, so the graph is `SourceRejected`). The parent's 2,584 / 260 / 4 was never pinned; the move is a
/// contents slot and one record lookup (`ri-ema.md`).
#[test]
fn r1_keep_list_parameter_without_a_call_is_pinned() {
    pin_form(
        "rfc_cases::list_contents::r1_keep_list_parameter_without_a_call_is_pinned",
        const {
            Case {
                text: DIRECT,
                typed_stats: (330577, 48520, 12),
                graph: (GraphCompletion::SourceRejected, (194154, 5367, 6)),
                roots: &[
                    rejected(
                        "local",
                        R0202_LOCAL.0,
                        R0202_LOCAL.1,
                        R0202_LOCAL.2,
                        (3011, 360, 6),
                    ),
                    checked("keep", (2760, 305, 4)),
                ],
            }
        },
        &[(
            "keep",
            &[
                "0: init user.id; goto 1",
                "1: init user; goto 2",
                "2: borrow user -> view; goto 3",
                "3: goto 4",
                "4: end view; end user.id; end user; goto 5",
                "5: end dst: &mut List<&User>; goto 6",
                "6: return",
            ],
        )],
    );
}

/// Condition 2: the loop form's rejecting twin. A `User` created inside the
/// loop body and retained into `dst` cannot outlive the iteration. When the
/// body ends first the finding is R0203 at the body block (as for
/// `stored_source_loans_reach_the_caller_view`'s `early`: the scope's end
/// releases a loan the destination still needs); when the body leaves the
/// function by `return` while the destination holds it, the finding is R0202
/// at the call, related the local's borrow. `looped` (a parameter source)
/// checks beside them.
#[test]
fn r1_loop_body_local_source_retained_into_dst_rejects() {
    installed(
        "rfc_cases::list_contents::r1_loop_body_local_source_retained_into_dst_rejects",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: LOOP_LOCAL,
                        typed_stats: (332471, 49168, 12),
                        graph: (GraphCompletion::SourceRejected, (196274, 5223, 8)),
                        roots: &[
                            checked("looped", (3546, 355, 7)),
                            rejected(
                                "loopLocal",
                                "R0203",
                                r####"{\n    let user = User { id: 1 };\n    retain(dst, &user);\n  }"####,
                                "&user",
                                (3960, 444, 8),
                            ),
                            rejected(
                                "loopLocalReturn",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (3936, 441, 8),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Condition 3 (RFC 0007 line 483, "Ending the call loan does not erase the
/// stored source"): after `retain(dst, &user)` returns, the call's own loan
/// is over but the source is still held by `dst`, which lives to the end of
/// the function. A write to the source in the next statement needs it
/// exclusively and rejects, R0203 at the write, related the borrow; an
/// ordinary view's loan ends at its last use, so the same write after the
/// view's last read checks (`viewReadThenWrite`). The write before the call
/// and a read after it reject at the call (R0202, a body-local owner stored
/// into a parameter's list): only the write after the call reaches the
/// loan-outlives-the-call judgment. Stated limit: a use that needs a
/// caller-provided source released or exclusively borrowed is not
/// expressible today (an `&mut User` source is typed `Incomplete`, a
/// caller-owned list is `Incomplete` at its call), so this judgment is
/// reached through a body-local owner only; see `ri-ema.md`.
#[test]
fn r1_stored_source_outlives_the_call_loan() {
    installed(
        "rfc_cases::list_contents::r1_stored_source_outlives_the_call_loan",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: OUTLIVES,
                        typed_stats: (335874, 49622, 12),
                        graph: (GraphCompletion::SourceRejected, (198616, 5538, 6)),
                        roots: &[
                            rejected(
                                "writeAfter",
                                "R0203",
                                "user.id = 2;",
                                "&user",
                                (3924, 431, 6),
                            ),
                            rejected(
                                "readAfter",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (3909, 422, 6),
                            ),
                            rejected(
                                "writeBefore",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (4006, 472, 6),
                            ),
                            checked("viewReadThenWrite", (2780, 346, 4)),
                        ],
                    }
                }],
            );
        },
    );
}

/// RFC 0007 R1: "After the call, the contents of `dst` carry the sources of
/// that input and their old contents" (review ci-0mdr SP-1). A body-local
/// source stored first is still in the contents after a later retain of a
/// parameter, directly or through a forwarding function, so each is R0202; a
/// strong update (the second call replacing the first source) would check
/// both. `forward` (a parameter alone) checks beside them.
#[test]
fn r1_old_contents_survive_a_later_retain() {
    installed(
        "rfc_cases::list_contents::r1_old_contents_survive_a_later_retain",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: OLD_CONTENTS,
                        typed_stats: (333498, 49416, 12),
                        graph: (GraphCompletion::SourceRejected, (197880, 5390, 6)),
                        roots: &[
                            checked("forward", (3384, 270, 5)),
                            rejected(
                                "reversed",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (5130, 572, 6),
                            ),
                            rejected(
                                "reversedViaCall",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (4360, 572, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Review ci-0mdr SP-6, re-review ci-b4b2 C-3, ticket ri-i0kj. The R0202 for a
/// body-local source stored into `dst` is reported at the call that stored the
/// local, with the body-local borrow as the related site, not at a later call
/// that retains into `dst` (RFC 0007 line 218). `paramThenLocal` stores the
/// local last (site `retain(dst, &user)`); `localThenParam` stores it first and
/// a valid parameter call follows, and the site is still `retain(dst, &user)`.
/// Both reject.
#[test]
fn r1_local_then_parameter_reports_the_storing_call() {
    installed(
        "rfc_cases::list_contents::r1_local_then_parameter_reports_the_storing_call",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: ORDER,
                        typed_stats: (331754, 49227, 12),
                        graph: (GraphCompletion::SourceRejected, (196271, 5172, 6)),
                        roots: &[
                            rejected(
                                "paramThenLocal",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (5108, 561, 6),
                            ),
                            rejected(
                                "localThenParam",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (5104, 559, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Ticket ri-i0kj. Two body-local sources stored into `dst`: R0202 is at the
/// first storing call, with that call's own borrow as the related site, in
/// either declaration order. RFC 0007 line 218 has the exit check reject a
/// body-local source that flows to `dst`; the earliest store is where the
/// contents first hold one, and the exit reports one finding per root. A valid
/// parameter call after the stores, in or out of a loop, is never the site.
#[test]
fn r1_two_local_stores_report_the_first_storing_call() {
    installed(
        "rfc_cases::list_contents::r1_two_local_stores_report_the_first_storing_call",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: TWO_LOCALS,
                        typed_stats: (343305, 51567, 12),
                        graph: (GraphCompletion::SourceRejected, (210460, 7464, 8)),
                        roots: &[
                            rejected(
                                "firstThenSecond",
                                "R0202",
                                "retain(dst, &one)",
                                "&one",
                                (5785, 778, 6),
                            ),
                            rejected(
                                "secondThenFirst",
                                "R0202",
                                "retain(dst, &two)",
                                "&two",
                                (5786, 778, 6),
                            ),
                            rejected(
                                "twoThenParam",
                                "R0202",
                                "retain(dst, &two)",
                                "&two",
                                (7309, 1087, 6),
                            ),
                            rejected(
                                "loopLocalThenParam",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (6065, 1037, 8),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Ticket ri-i0kj, review ci-o92c F1. A view of a body-local stored on both
/// branches of an `if` reaches the join as two states that differ only in the
/// storing call; they stay apart so that each path names its own call. R0202
/// is at the first branch's call (`retain(dst, view)`), not at the later valid
/// `retain(dst, a)`. `v37_viewsOfTwo` stores a different local on each branch
/// and reports the first branch's. Both are exact and one short.
#[test]
fn r1_view_stored_on_two_branches_reports_the_first_branch_call() {
    installed(
        "rfc_cases::list_contents::r1_view_stored_on_two_branches_reports_the_first_branch_call",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: VIEW_BRANCHES,
                        typed_stats: (335938, 50247, 12),
                        graph: (GraphCompletion::SourceRejected, (208001, 8132, 7)),
                        roots: &[
                            rejected(
                                "v23_viewBranches",
                                "R0202",
                                "retain(dst, view)",
                                "&user",
                                (7303, 1228, 7),
                            ),
                            rejected(
                                "v37_viewsOfTwo",
                                "R0202",
                                "retain(dst, v)",
                                "&two",
                                (12545, 2888, 7),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Metering rules 7 and 11 (third case, ruling of 2026-11-24 on review
/// ci-o92c F1), ticket ri-i0kj. `Held::local_store` is part of the state, so
/// two branches that store the same view at different calls stay two states
/// until a strong update retires them. `Checked` programs pay for that: the
/// split roots below cost more than at the parent (`d85303bd`) and the control,
/// which has no split, costs the same. Each root is pinned exact and one short
/// and each parent cap now gives `ResourceLimit`, in both modes. The old and
/// new numbers are in `m5-acceptance/ri-i0kj.md`.
#[test]
fn r1_split_stores_that_repair_cost_more_and_their_parent_caps_give_resource_limit() {
    installed(
        "rfc_cases::list_contents::r1_split_stores_that_repair_cost_more_and_their_parent_caps_give_resource_limit",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: SPLIT_STORES,
                        typed_stats: (340822, 50456, 12),
                        graph: (GraphCompletion::Incomplete, (220698, 15030, 9)),
                        roots: &[
                            checked("c1_splitThenRepair", (12798, 3765, 7)),
                            checked("c2_split3ThenRepair", (16847, 5686, 9)),
                            checked("c3_noSplitControl", (11529, 3074, 7)),
                        ],
                    }
                }],
            );
            old_root_caps(
                &request,
                &[
                    (
                        SPLIT_STORES,
                        &[
                            ("c1_splitThenRepair", (11439, 3030, 7)),
                            ("c2_split3ThenRepair", (13264, 3976, 9)),
                        ],
                        (214952, 12549, 9),
                    ),
                    (
                        VIEW_BRANCHES,
                        &[
                            ("v23_viewBranches", (7198, 1162, 7)),
                            ("v37_viewsOfTwo", (11991, 2576, 7)),
                        ],
                        (207342, 7754, 7),
                    ),
                ],
            );
        },
    );
}

/// Ticket ri-i0kj, review ci-o92c F2. A body-local source stored by assignment
/// through the destination is reported at the assignment, as a store by call
/// is reported at the call: a later valid call (`set(slot, input)`,
/// `remember(slot, a)`) is never the site. `o5` overwrites a call's store with
/// an assignment, so the site is the assignment that holds the local now;
/// `o7` stores by assignment first and a second local by call, and the first
/// store is the site. `oa` stores by assignment alone, as at the parent.
#[test]
fn r1_assignment_stores_report_the_assigning_statement() {
    installed(
        "rfc_cases::list_contents::r1_assignment_stores_report_the_assigning_statement",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: ASSIGNED_STORES,
                        typed_stats: (340968, 49757, 12),
                        graph: (GraphCompletion::SourceRejected, (202445, 7014, 6)),
                        roots: &[
                            rejected(
                                "o3_assignLocalThenCall",
                                "R0202",
                                "*slot = &flag;",
                                "&flag",
                                (2716, 436, 5),
                            ),
                            rejected(
                                "r4_assignLocalThenCall",
                                "R0202",
                                "*slot = &p;",
                                "&p",
                                (3702, 596, 5),
                            ),
                            rejected(
                                "o8_branchAssign",
                                "R0202",
                                "*slot = &flag;",
                                "&flag",
                                (3157, 670, 6),
                            ),
                            rejected(
                                "o5_callLocalAssignLocalCall",
                                "R0202",
                                "*slot = &other;",
                                "&other",
                                (3264, 706, 6),
                            ),
                            rejected(
                                "o7_assignLocalCallLocalCall",
                                "R0202",
                                "*slot = &flag;",
                                "&flag",
                                (3276, 708, 6),
                            ),
                            rejected(
                                "oa_assignLocalOnly",
                                "R0202",
                                "*slot = &flag;",
                                "&flag",
                                (2398, 290, 4),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Metering rule 7, ticket ri-i0kj. The storing site is a premise of the
/// state, and each root changes it between two uses of the same loans (one
/// local view, `&flag`). `repaired` stores at `set`, overwrites the contents
/// with a valid source and stores the same view again at `keep`: the site is
/// `keep`, not the stale `set`. `joinedSetFirst` and `joinedKeepFirst` differ
/// only in which call each branch makes: the first branch's call is the site
/// in both, and the valid `set(slot, input)` after the join is not.
#[test]
fn r1_the_storing_call_is_a_premise_of_the_state() {
    installed(
        "rfc_cases::list_contents::r1_the_storing_call_is_a_premise_of_the_state",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: STORE_PREMISES,
                        typed_stats: (334232, 49127, 12),
                        graph: (GraphCompletion::SourceRejected, (198422, 7001, 7)),
                        roots: &[
                            rejected(
                                "repaired",
                                "R0202",
                                "keep(slot, view)",
                                "&flag",
                                (3190, 659, 5),
                            ),
                            rejected(
                                "joinedSetFirst",
                                "R0202",
                                "set(slot, view)",
                                "&flag",
                                (4022, 1099, 7),
                            ),
                            rejected(
                                "joinedKeepFirst",
                                "R0202",
                                "keep(slot, view)",
                                "&flag",
                                (4022, 1099, 7),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// RFC 0007 R1: exits "must not retain body-local owners, temporary storage or
/// destroyed owned-parameter storage" (review ci-0mdr SP-2). A by-value
/// parameter's storage ends with the call, so retaining a view of it into
/// `dst` is R0202 directly, through a local view, on one branch, in a loop
/// whose condition can fail and in a loop that breaks. A temporary source (a
/// call result or a literal) is not expressible today: the targeted query is
/// `Incomplete` at the temporary, so no unchecked shape reaches `dst` (a stated
/// limit, the O12 call-result rule has no list destination).
#[test]
fn r1_owned_parameter_and_temporary_sources_do_not_escape() {
    installed(
        "rfc_cases::list_contents::r1_owned_parameter_and_temporary_sources_do_not_escape",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: OWNED_SOURCES,
                        typed_stats: (338772, 50101, 12),
                        graph: (GraphCompletion::SourceRejected, (201987, 5871, 8)),
                        roots: &[
                            rejected(
                                "byValue",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (3689, 314, 6),
                            ),
                            rejected(
                                "hop",
                                "R0202",
                                "retain(dst, view)",
                                "&owned",
                                (3811, 372, 5),
                            ),
                            rejected(
                                "oneBranch",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (3974, 459, 8),
                            ),
                            rejected(
                                "loopOwned",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (4234, 584, 8),
                            ),
                            rejected(
                                "loopBreak",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (3809, 376, 8),
                            ),
                        ],
                    }
                }],
            );
            check_cases(
                &request,
                &[const {
                    Case {
                        text: TEMPORARY_SOURCES,
                        typed_stats: (330879, 48738, 12),
                        graph: (GraphCompletion::Incomplete, (191890, 4301, 6)),
                        roots: &[
                            checked("make", (2363, 154, 4)),
                            incomplete("call", "make()", (2941, 63, 6)),
                            incomplete("literal", "User { id: 1 }", (2941, 63, 6)),
                        ],
                    }
                }],
            );
        },
    );
}

/// Rejecting twin of `r1_pair_of_user_sources_checks_with_its_sequence` (review
/// ci-0mdr SP-5, RFC 0007: a rule "never accepts an invalid program"): one
/// body-local source in a pair is R0202 at the call, whichever position holds it.
#[test]
fn r1_pair_with_one_body_local_source_rejects() {
    installed(
        "rfc_cases::list_contents::r1_pair_with_one_body_local_source_rejects",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: PAIR_LOCAL,
                        typed_stats: (330804, 49095, 12),
                        graph: (GraphCompletion::SourceRejected, (193688, 4884, 6)),
                        roots: &[
                            rejected(
                                "localSecond",
                                "R0202",
                                "retain(dst, a, &user)",
                                "&user",
                                (3901, 415, 6),
                            ),
                            rejected(
                                "localFirst",
                                "R0202",
                                "retain(dst, &user, a)",
                                "&user",
                                (3899, 414, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Rejecting twin of `r1_lone_destination_checks_with_its_sequence` (review
/// ci-0mdr SP-5): a lone-destination call adds no source but keeps "every
/// source they already held", so a body-local source stored before or after
/// it still reaches the exit. R0202 is at the storing call, whether `touch(dst)`
/// comes before or after it.
#[test]
fn r1_lone_destination_keeps_a_stored_local_rejected() {
    installed(
        "rfc_cases::list_contents::r1_lone_destination_keeps_a_stored_local_rejected",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: LONE_LOCAL,
                        typed_stats: (333422, 49207, 12),
                        graph: (GraphCompletion::SourceRejected, (197402, 5006, 6)),
                        roots: &[
                            rejected(
                                "loneAfterLocal",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (4951, 475, 6),
                            ),
                            rejected(
                                "loneBeforeLocal",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (4960, 478, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// The stated limit of condition 3, pinned: a caller-provided `&mut User`
/// source cannot reach `retain` today. The typed pass stops (`Incomplete`, no
/// receipt, so the ownership pass never sees the call), for the reborrow
/// `&*source` and for passing `source` itself; a use that needs a
/// caller-provided source exclusively while `dst` is live is not expressible.
/// Rule 8: the outcome is `Incomplete` at exact typed caps, no ownership
/// receipt is published, and one short on work gives `ResourceLimit`. The
/// typed report names the stop in its Result-use pending list (review
/// ci-0mdr ST-10): the write `source.id = 2;` (`Storage`) for the reborrow,
/// and for passing `source` itself the call (`TypedInput`, with body finding
/// R0300 at the call: the `&mut User` argument does not meet the `&User`
/// parameter) and the write. It has no body obligation and no core blocker.
#[test]
fn r1_caller_provided_exclusive_source_stops_typed() {
    installed(
        "rfc_cases::list_contents::r1_caller_provided_exclusive_source_stops_typed",
        |request| {
            for (module, stats, pending, findings) in [
                (
                    EXCLUSIVE_SOURCE,
                    (326346, 48099, 12),
                    &[(ResultUseBlocker::Storage, "source.id = 2;")][..],
                    &[][..],
                ),
                (
                    EXCLUSIVE_PASSED,
                    (325879, 47888, 12),
                    &[
                        (ResultUseBlocker::TypedInput, "retain(dst, source)"),
                        (ResultUseBlocker::Storage, "source.id = 2;"),
                    ][..],
                    &[("R0300", "retain(dst, source)")][..],
                ),
            ] {
                fs::write(&request.entry, format!("{IMPORTS}\n{module}")).unwrap();
                for test_mode in [false, true] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let graph = loader::load_installed(request).unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    assert_eq!(typed.completion(), TypedCompletion::Incomplete);
                    assert_eq!(typed.statistics(), statistics(stats));
                    assert!(typed.receipt().is_none());
                    let blockers: Vec<_> = typed
                        .result_use()
                        .unwrap()
                        .pending()
                        .iter()
                        .map(|p| (p.blocker, text(&p.site)))
                        .collect();
                    let pending: Vec<_> =
                        pending.iter().map(|(b, s)| (*b, s.to_string())).collect();
                    assert_eq!(blockers, pending);
                    let body_findings: Vec<_> = typed
                        .bodies()
                        .findings()
                        .iter()
                        .map(|f| (f.kind.code().unwrap(), text(&f.site)))
                        .collect();
                    let findings: Vec<_> =
                        findings.iter().map(|(c, s)| (*c, s.to_string())).collect();
                    assert_eq!(body_findings, findings);
                    let (exact, short) = limits(statistics(stats));
                    let replay = analyze_typed(&graph, exact);
                    assert_eq!(replay.completion(), TypedCompletion::Incomplete);
                    assert_eq!(replay.statistics(), typed.statistics());
                    for short in short {
                        let limited = analyze_typed(&graph, short);
                        crate::limit::assert_typed_limit(&limited);
                    }
                }
            }
        },
    );
}

/// One module, its caps (exact and one short, both modes, the targeted query
/// and the ordinary graph root: `check_cases`) and, for each named `Checked`
/// root, the whole MIR sequence of the targeted query and of the graph
/// root's ordinary MIR (rule 9).
fn pin_form(test: &'static str, case: Case, sequences: &[(&str, &[&str])]) {
    installed(test, |request| {
        check_cases(&request, std::slice::from_ref(&case));
        fs::write(&request.entry, format!("{IMPORTS}\n{}", case.text)).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            let receipt = typed.receipt().unwrap();
            let module = super::o12::application(&graph);
            let whole = analyze_ownership(&receipt, AnalysisLimits::default());
            for (name, expected) in sequences {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some(*name)
                    })
                    .unwrap();
                let targeted =
                    lower_body(&receipt, module, &declaration.id, AnalysisLimits::default())
                        .unwrap();
                assert_eq!(targeted.completion(), Completion::Checked, "{name}");
                assert_eq!(&sequence(targeted.mir().unwrap()), expected, "{name}");
                let root = whole
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == declaration.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                assert_eq!(&sequence(root.ordinary_mir().unwrap()), expected, "{name}");
            }
        }
    });
}

/// A source, the parent's cap of each named root and of the whole graph.
pub(super) type OldCaps<'a> = (&'a str, &'a [(&'a str, Triple)], Triple);

/// Metering rule 11: each `(source, roots, graph)` is the parent's exact cap
/// (`b6c93f30`). With the same source, every old cap now gives
/// `ResourceLimit` (no MIR), in both modes.
pub(super) fn old_root_caps(request: &InstalledRequest, olds: &[OldCaps<'_>]) {
    for (text, roots, whole) in olds {
        fs::write(&request.entry, format!("{IMPORTS}\n{text}")).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            let receipt = typed.receipt().unwrap();
            let module = super::o12::application(&graph);
            let (old, _) = limits(statistics(*whole));
            crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
            for (name, old) in *roots {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| match *name {
                        "expect" => d.id.kind == DeclarationKind::Expectation,
                        name => {
                            d.id.kind == DeclarationKind::Function
                                && d.name.as_deref() == Some(name)
                        }
                    })
                    .unwrap();
                let (old, _) = limits(statistics(*old));
                let limited = if *name == "expect" {
                    lower_root(
                        &receipt,
                        module,
                        compiler::analysis::scalars::BodyOwner::Declaration(&declaration.id),
                        old,
                    )
                } else {
                    lower_body(&receipt, module, &declaration.id, old)
                }
                .unwrap();
                crate::limit::assert_body_limit(&limited);
            }
        }
    }
}

fn place(place: &Place<'_>) -> String {
    let root = String::from_utf8_lossy(place.root.span.bytes()).into_owned();
    match place.field() {
        Some(field) => format!("{root}.{}", field.name.as_deref().unwrap()),
        None => root,
    }
}

fn destination(destination: &RetainedPlace<'_>) -> String {
    match destination {
        RetainedPlace::Local(value) => format!("local {}", place(value)),
        RetainedPlace::ThroughParameter(value) => format!("through {}", place(value)),
        other => panic!("unexpected destination {other:?}"),
    }
}

fn terminator(terminator: &Terminator) -> String {
    match terminator {
        Terminator::Goto(next) => format!("goto {next}"),
        Terminator::Branch { yes, no } => format!("branch {yes} {no}"),
        Terminator::Return => "return".to_string(),
        other => panic!("unexpected terminator {other:?}"),
    }
}

/// The whole checked MIR, one entry per block that does something (its index,
/// its action and its terminator; a block that only jumps is its terminator),
/// in block order: straight lines and loops alike (rule 9).
pub(super) fn sequence(mir: &Mir<'_>) -> Vec<String> {
    let mut events = Vec::new();
    for (index, block) in mir.blocks.iter().enumerate() {
        let action = match &block.operation {
            Operation::None => String::new(),
            Operation::Initialize(value) => format!("init {}", place(value)),
            Operation::Copy {
                from,
                into: Some(into),
            } => format!("copy {} -> {}", place(from), place(into)),
            Operation::Reborrow {
                parent,
                holder,
                exclusive,
            } => format!(
                "reborrow{} {} -> {}",
                if *exclusive { " mut" } else { "" },
                place(parent),
                place(holder)
            ),
            Operation::Borrow {
                source,
                holder,
                exclusive,
            } => format!(
                "borrow{} {} -> {}",
                if *exclusive { " mut" } else { "" },
                place(source),
                place(holder)
            ),
            Operation::CallRetaining {
                destination: to,
                input,
                ..
            } => format!("retain {} <- {}", destination(to), place(input)),
            Operation::CallRetainingNoInput {
                destination: to, ..
            } => format!("retain {} <- nothing", destination(to)),
            Operation::CallRetainingPair {
                destination: to,
                inputs,
                ..
            } => format!(
                "retain {} <- {}, {}",
                destination(to),
                place(&inputs[0]),
                place(&inputs[1])
            ),
            Operation::Dereference(value) => format!("deref {}", place(value)),
            Operation::Export(value) => format!("export {}", place(value)),
            Operation::Cleanup(actions) => {
                let ends: Vec<_> = actions
                    .iter()
                    .map(|action| match action {
                        Obligation::EndStorage(value) => format!("end {}", place(value)),
                        other => panic!("unexpected cleanup {other:?}"),
                    })
                    .collect();
                ends.join("; ")
            }
            other => panic!("unexpected operation {}", &format!("{other:?}")[..40]),
        };
        let line = if action.is_empty() {
            terminator(&block.terminator)
        } else {
            format!("{action}; {}", terminator(&block.terminator))
        };
        events.push(format!("{index}: {line}"));
    }
    events
}

/// ST-3 (review ci-0mdr, ri-px1.11.4): a `List<&Record>` parameter registers
/// its slot in `locals`, a map keyed by a `DeclarationId` that ends in a
/// module path, so the insert costs `path.len() + 1` per probe (metering
/// rule 3, `charge_key`). The third parameter probes a map of two locals.
const LIST_REFERENCE_PARAMETERS: &str = r####"
type User = { id: i32 };
function keep(users: List<&User>): void { }
function keepThree(a: List<&User>, b: List<&User>, c: List<&User>): void { }
"####;

#[test]
fn list_reference_parameter_charges_its_insert() {
    installed(
        "rfc_cases::list_contents::list_reference_parameter_charges_its_insert",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: LIST_REFERENCE_PARAMETERS,
                        typed_stats: (324495, 47725, 12),
                        graph: (GraphCompletion::Incomplete, (186673, 4198, 5)),
                        roots: &[
                            checked("keep", (1922, 44, 1)),
                            checked("keepThree", (2025, 68, 1)),
                        ],
                    }
                }],
            );
            old_root_caps(
                &request,
                &[(
                    LIST_REFERENCE_PARAMETERS,
                    &[("keep", (1912, 44, 1)), ("keepThree", (1985, 68, 1))],
                    (186623, 4198, 5),
                )],
            );
        },
    );
}

/// Metering rule 11, mandated charge (ri-px1.11.4, ST-3): the field read
/// `source.id` after the call probes `locals` through `charge_key`, so the
/// roots below moved. Each parent cap (`eabe14ad`) now gives `ResourceLimit`,
/// in both modes. `m5-acceptance/ri-px1.11.4.md` holds old, new and reason.
#[test]
fn list_roots_pre_charge_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::list_contents::list_roots_pre_charge_caps_now_give_resource_limit",
        |request| {
            old_root_caps(
                &request,
                &[
                    (
                        AFTER_RETAIN,
                        &[("reuse", (3489, 322, 5))],
                        (193669, 4807, 5),
                    ),
                    (AFTER_READ, &[("reuse", (3457, 306, 5))], (189806, 4408, 5)),
                    (
                        OUTLIVES,
                        &[("viewReadThenWrite", (2761, 346, 4))],
                        (198597, 5538, 6),
                    ),
                ],
            );
        },
    );
}

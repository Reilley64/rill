use super::*;
use crate::analysis::typed::analyze_typed;
use crate::loader::{self, InstalledRequest, Origin};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SOURCE: &str = r#"
import std::{List, Result, Err};
import std::channel::{Receiver, Sender};
import std::convert::{into, tryInto};
import std::error::formatError;
import std::iter::iterate;
import std::mutex::LockGuard;
import std::number::ParseError;
import std::option::{Option, map as mapOption};
import std::result::{isOk, map as mapResult};
import std::shared::Shared;

function runTwo<W, R>(jobs: W): R;
implement<F, G, A, B> runTwo<[F, G], [A, B]>(jobs: [F, G]): [A, B]
where callableOnce(F, [], A), callableOnce(G, [], B), sendable(F), sendable(G), sendable(A), sendable(B) {
    return [jobs[0](), jobs[1]()];
}
function later(): void {}
function parent(input: List<i32>): [i32, i32] {
    let numbers = input;
    let [a, b] = runTwo([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
    dispose(numbers);
    later();
    return [a, b];
}
"#;

// The ordinary lowering supplies only the reached input state and checked
// frame bodies. None of its synchronous invocations are advanced as children.
pub(in crate::analysis::ownership) struct Probe<'g> {
    later: &'g DeclarationId,
    published_pending: bool,
    later_exit: Option<CancellationExit<'g>>,
}

impl<'g> Probe<'g> {
    pub(in crate::analysis::ownership::flow) fn observe(
        &mut self,
        parent: &CancellationFrame<'_, 'g>,
        (input, output): (&[State], &[State]),
        action: Action<'g>,
        operation: &Operation<'g>,
        site: Site<'g>,
    ) {
        let mut budget = budget(AnalysisLimits::default());
        match (action, operation) {
            (
                Action::CallListArrows {
                    receivers, into, ..
                },
                Operation::CallListArrows { slots, .. },
            ) => {
                assert!(!self.published_pending);
                let stage = |budget: &mut Budget| {
                    Staged::check(parent, input, receivers, into, slots, site, budget)
                };
                check_stop(checked(stage(&mut budget)), &mut budget);
                for (order, masked_stop) in [([0, 1], false), ([1, 0], true)] {
                    let proof = checked(stage(&mut budget));
                    let (results, pending, parent_state) =
                        check_publish(proof, order, masked_stop, &mut budget);
                    assert_eq!(
                        parent_state, output,
                        "only the published tuple can enter the suffix"
                    );
                    for (i, result) in results.into_iter().enumerate() {
                        let expected: [&[u8]; 2] = [b"(&numbers)[0u64]", b"(&numbers)[1u64]"];
                        assert_eq!(result.from.root.span.bytes(), expected[i]);
                        assert_eq!(result.into.tuple_element(), Some(i));
                    }
                    self.published_pending = pending;
                }
                check_bounds(stage);
            }
            (_, Operation::Call(callee)) if std::ptr::eq(*callee, self.later) => {
                // The masked request survived publication and the normal
                // suffix disposed numbers before reaching this enabled entry.
                assert!(self.published_pending);
                assert!(checked(parent.check(input, site.node, &mut budget)));
                self.later_exit = Some(checked(parent.exit(
                    Cow::Borrowed(input),
                    site.node,
                    &mut budget,
                )));
            }
            _ => {}
        }
    }
}

fn budget(limits: AnalysisLimits) -> Budget {
    Budget {
        limits,
        statistics: Statistics::default(),
    }
}

fn checked<T>(result: ResultOf<T>) -> T {
    result.unwrap_or_else(|stop| match stop {
        Stop::Pending(node) => panic!("pending at {node}"),
        Stop::Rejected {
            site,
            related,
            kind,
        } => panic!("rejected at {site}, {related}: {kind:?}"),
        Stop::Limit => panic!("resource limit"),
    })
}

fn wait<'g>(state: Box<Waiting<'g>>, event: Event, budget: &mut Budget) -> Box<Waiting<'g>> {
    let Successor::Wait(next) = checked(state.advance(event, budget)) else {
        panic!("a child still owes execution or cleanup");
    };
    next
}

use ChildEvent::*;
use Event::*;

fn check_stop(proof: Staged<'_>, budget: &mut Budget) {
    let state = checked(proof.commit(budget));
    let state = wait(state, Child(0, Enter), budget);
    let state = wait(state, Child(0, Export), budget);
    let state = wait(state, Child(0, CleanupCompleted), budget);
    let state = wait(state, RequestStop, budget);
    let state = wait(state, HandleDrop, budget);
    assert!(!state.stopping && state.pending);
    assert_eq!(state.stop_requested, [false, false]);
    let state = wait(state, EnabledYield, budget);
    assert_eq!(state.stop_requested, [false, true]);
    let state = wait(state, Child(1, EntryDelivery), budget);
    let state = wait(state, RequestStop, budget);
    let state = wait(state, MaskedYield, budget);
    let state = wait(state, EnabledYield, budget); // internal deferral wins
    assert_eq!(state.stop_requested, [false, true]);
    let [
        ChildState::Complete,
        ChildState::Cleaning {
            environment: held,
            end_view: Cleanup::EndStorage(view),
            entry_delivery: Some(entry),
        },
    ] = &state.children
    else {
        panic!("entry delivery owes real view cleanup")
    };
    assert_eq!(held.loans.len(), 1);
    assert_eq!(entry.span.bytes(), b"(&numbers)[1u64]");
    assert_eq!(view.root.span.bytes(), b"&numbers");
    assert!(state.stopping && state.pending);
    assert!(matches!(state.results, [Some(_), None]));
    assert!(
        state
            .backings()
            .iter()
            .all(|backing| backing.root.span.bytes() == b"numbers")
    );
    let Successor::Cancel {
        retired: [Some(retired), None],
        parent_exit,
    } = checked(state.advance(Child(1, CleanupCompleted), budget))
    else {
        panic!("only the initialized, unpublished result is retired");
    };
    assert_eq!(retired.into.tuple_element(), Some(0));
    assert!(
        matches!(parent_exit.cleanup.as_slice(), [Cleanup::DisposeListI32 { operand, .. }]
        if operand.root.span.bytes() == b"numbers")
    );
}

fn check_publish<'g>(
    proof: Staged<'g>,
    [first, second]: [usize; 2],
    masked_stop: bool,
    budget: &mut Budget,
) -> ([ScalarExport<'g>; 2], bool, Vec<State>) {
    let receivers = proof.receivers;
    let into = proof.into;
    assert!(
        receivers
            .iter()
            .all(|&i| matches!(proof.parent_state[i], State::Initialized(_)))
    );
    let state = checked(proof.commit(budget));
    assert!(
        receivers
            .iter()
            .all(|&i| matches!(state.proof.parent_state[i], State::Moved(_)))
    );
    let state = wait(state, Child(first, Enter), budget);
    let state = wait(state, Child(second, Enter), budget);
    let state = wait(state, Child(first, Export), budget);
    let state = wait(state, Child(first, CleanupCompleted), budget);
    let state = wait(state, Child(second, Export), budget);
    assert!(matches!(
        state.children[second],
        ChildState::Cleaning {
            end_view: Cleanup::EndStorage(_),
            entry_delivery: None,
            ..
        }
    ));
    assert!(state.results.iter().all(Option::is_some));
    assert!(
        state.proof.parent_state[into..into + 3]
            .iter()
            .all(|slot| *slot == State::Vacant)
    );
    assert!(
        state
            .backings()
            .iter()
            .all(|backing| backing.root.span.bytes() == b"numbers")
    );
    let state = if masked_stop {
        let state = wait(state, RequestStop, budget);
        wait(state, MaskedYield, budget)
    } else {
        wait(state, EnabledYield, budget)
    };
    assert!(!state.stopping);
    let Successor::Publish {
        results,
        pending,
        parent_state,
    } = checked(state.advance(Child(second, CleanupCompleted), budget))
    else {
        panic!("both completed children publish on the ordinary path")
    };
    assert_eq!(pending, masked_stop);
    (results, pending, parent_state)
}

fn check_bounds<'g>(stage: impl Fn(&mut Budget) -> ResultOf<Staged<'g>>) {
    let replay = |budget: &mut Budget| -> ResultOf<Successor<'g>> {
        let mut state = stage(budget)?.commit(budget)?;
        for event in [
            Child(0, Enter),
            Child(0, Export),
            Child(0, CleanupCompleted),
            RequestStop,
            EnabledYield,
            Child(1, EntryDelivery),
            MaskedYield,
        ] {
            let Successor::Wait(next) = state.advance(event, budget)? else {
                panic!("premature terminal edge");
            };
            state = next;
        }
        state.advance(Child(1, CleanupCompleted), budget)
    };
    let mut check = budget(AnalysisLimits::default());
    assert!(matches!(
        checked(replay(&mut check)),
        Successor::Cancel { .. }
    ));
    let Statistics {
        work,
        entries,
        max_depth,
    } = check.statistics;
    let exact = AnalysisLimits {
        max_work: work,
        max_entries: entries,
        max_depth,
    };
    assert!(matches!(
        checked(replay(&mut budget(exact))),
        Successor::Cancel { .. }
    ));
    for short in [
        AnalysisLimits {
            max_work: work - 1,
            ..exact
        },
        AnalysisLimits {
            max_entries: entries - 1,
            ..exact
        },
        AnalysisLimits {
            max_depth: max_depth - 1,
            ..exact
        },
    ] {
        assert!(matches!(replay(&mut budget(short)), Err(Stop::Limit)));
    }
    for event in [
        Child(2, Enter),
        Child(0, EntryDelivery),
        Child(0, CleanupCompleted),
    ] {
        let state = checked(checked(stage(&mut check)).commit(&mut check));
        assert!(matches!(
            state.advance(event, &mut check),
            Err(Stop::Pending(_))
        ));
    }
}

#[test]
fn real_frames_drive_the_unregistered_scoped_join() {
    if let Some(root) = std::env::var_os("RILL_JOIN_PROBE_ROOT") {
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                entry: PathBuf::from(&root).join("main.rill"),
                target: "aarch64-apple-darwin".into(),
                options: Default::default(),
                test_mode,
                limits: Default::default(),
            })
            .unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            assert!(typed.receipt().is_some(), "{:?}", typed.completion());
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();
            let header = |name| {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                &typed.inventory().type_facts().declarations[&declaration.id]
            };
            let mut budget = budget(AnalysisLimits::default());
            let mut cache = QueryCache::default();
            let later = header("later");
            let leaf = checked(lower(
                &typed,
                module,
                later,
                Frame::OrdinaryFunction,
                true,
                None,
                Lowering::new(&mut budget, &mut cache),
            ));
            assert!(leaf.empty_entry.is_some());
            cache.join_probe = Some(Probe {
                later: &later.declaration.id,
                published_pending: false,
                later_exit: None,
            });
            let root = checked(lower(
                &typed,
                module,
                header("parent"),
                Frame::OrdinaryFunction,
                true,
                None,
                Lowering::new(&mut budget, &mut cache),
            ));
            assert!(root.mir.cancellation().is_none());
            let mut formed = Vec::new();
            let mut pc = root.mir.entry;
            loop {
                let block = &root.mir.blocks[pc];
                if let Operation::FormClosure { closure, .. } = block.operation {
                    formed.push(closure.node);
                }
                if let Operation::CallListArrows { slots, .. } = &block.operation {
                    assert_eq!(
                        formed,
                        slots.each_ref().map(|slot| slot.environment.root.node)
                    );
                    break;
                }
                let Terminator::Goto(next) = block.terminator else {
                    panic!("expected checked pair")
                };
                pc = next;
            }
            let probe = cache.join_probe.unwrap();
            let exit = probe
                .later_exit
                .expect("reached the independently checked later entry");
            assert!(
                matches!(exit.cleanup.as_slice(), [Cleanup::EndStorage(b), Cleanup::EndStorage(a)]
                if b.root.span.bytes() == b"b" && a.root.span.bytes() == b"a")
            );
        }
        return;
    }

    let root = std::env::temp_dir().join(format!("j{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Temp(&root);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib = root.join("lib/rill");
    let stdlib = lib.join("std");
    let runtime = lib.join("runtime/aarch64-apple-darwin");
    for path in [&stdlib, &runtime, &root.join("bin")] {
        fs::create_dir_all(path).unwrap();
    }
    for name in
        include_str!("../../../../../../../tests/installed/typed/stdlib-sources.txt").lines()
    {
        fs::copy(repo.join("stdlib").join(name), stdlib.join(name)).unwrap();
    }
    fs::copy(
        env!("RILL_OWNERSHIP_ARCHIVE"),
        runtime.join("librill_ownership.rlib"),
    )
    .unwrap();
    fs::copy(
        repo.join("runtime/error/src/lib.rs"),
        runtime.join("error.rs"),
    )
    .unwrap();
    fs::write(root.join("main.rill"), SOURCE).unwrap();
    let executable = root.join("bin/probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let test = concat!(
        module_path!(),
        "::real_frames_drive_the_unregistered_scoped_join"
    );
    let output = Command::new(executable)
        .env("RILL_JOIN_PROBE_ROOT", &root)
        .current_dir(&root)
        .args([
            "--exact",
            test.strip_prefix("compiler::").unwrap(),
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

struct Temp<'a>(&'a Path);
impl Drop for Temp<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}

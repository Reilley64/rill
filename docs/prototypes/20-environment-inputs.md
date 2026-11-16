# Environment and argument case inventory

Read the source examples in [`prototypes/20-environment-inputs.rill`](../../prototypes/20-environment-inputs.rill).

Design evidence for [#36](https://github.com/Reilley64/rill/issues/36), [ADR 0019](../adr/0019-owned-startup-environment-and-arguments.md) and the [public contract](../standard-library-contracts.md#12-environment-and-argument-inputs). Reviewers checked the companion's 29 intended-valid expectations and this independent inventory by hand, without compiling or executing them. This fixture delivers no native/WASI environment reader, startup runtime, decoder, library implementation or checker. #41's language-wide implement form applies to any future family bodies. The companion only defines standalone helpers and consumes canonical library functions.

## Positive source inventory

The companion covers empty inputs, full argument order including argument zero, empty arguments and an argument containing spaces left intact. Environment cases cover byte-name sorting, case-sensitive lookup, missing versus empty values, non-shell names and values containing equals. Other cases cover non-UTF-8 raw names/values, lookup-local decoding and bulk conversion positions.

Ownership cases use independently owned text and copies, copied constructor inputs and explicit configuration moved to a scoped worker. The companion also checks the empty default test snapshot. Typed rejection outcomes cover all synthetic name/argument/value validation classes, duplicates and first-error priority. The helpers do not observe private storage identity or prove the absence of hidden counting. Independence is the confirmed contract, not a proof inferred from disposal examples.

Two expectations query inputs() only in rill test. They require the agreed empty default and must not inspect the developer's real environment. The other cases use fromParts; no startup let is captured by a named function or expectation. Strings become owned Bytes through existing bytes/copy operations. The examples invent no string-to-Bytes constructor, conversion-family implementation or implicit view-to-owner cast. Inspect errors through standard value/kind/position without copying envelopes or parsing display text. All sizes and indices have explicit u64 contexts; byte literals have u8 contexts and worker markers use u32.

## Runtime result and ordering cases

| Case | Required outcome |
| --- | --- |
| Empty argument/environment lists | Valid Inputs; counts zero. |
| Argument zero is arbitrary text or missing | Preserve as supplied; no current-executable/path guarantee. |
| An argument contains spaces or quote characters | Preserve bytes; no second shell split/unquoting. |
| Missing valid variable | Ok(None), not an error or empty value. |
| Present empty variable | Ok(Some(empty bytes/text)). |
| Name 1.x-y or a non-UTF-8 name | Valid if nonempty and without NUL/equals. |
| Names A and a | Distinct; unsigned byte sort puts A first. |
| Prefix names A and AA | A sorts before AA, without locale or normalization. |
| Host entry A=x=y | The name is A and the value is x=y. Split only at the first equals. |
| Host entry without equals | MalformedEntry at VariableName(original index). This cannot be manufactured through already-paired fromParts. |
| Host entry beginning with equals | InvalidName, not a special hidden entry. |
| Duplicate exact name, identical or different values | Reject at the later VariableName(original index), no partial snapshot/first-wins/last-wins. |
| Argument NUL and invalid first environment entry | InvalidArgument first. |
| Invalid name and invalid value in the same entry | InvalidName first; name precedes value. |
| Valid duplicate name with NUL in its value | InvalidValue before duplicate detection. |
| Several independently invalid arguments/entries | First supplied-order failure before sorting. |
| Invalid lookup name on empty Inputs | InvalidName/LookupName, not Ok(None). |
| Missing non-UTF-8 byte query name | Valid byte lookup returning absence. |
| Raw queried value contains invalid UTF-8 | Byte query succeeds; text query returns InvalidUtf8 at its canonical variable index. |
| An unrelated name/value is non-UTF-8 | A valid text lookup still succeeds; acquisition itself does not require UTF-8. |
| Bulk args has an invalid item | First argument-order InvalidUtf8; no skipped/replaced entries or partial output. |
| Bulk vars has an invalid name and value | Report the first canonical variable's failure, checking its name before its value. |
| Variable supplied first but sorted last has invalid text | Text conversion position is its canonical index, not the old construction index. |
| Text result used after Inputs disposal | Valid independent owned string/list. |
| copy(&Inputs) then dispose original | Copy remains independently owned. |

## Independent source rejection cases

These are expected checker outcomes, not executable compile-fail fixtures:

1. Directly construct/rebuild Inputs or EnvError private state, or obtain mutable access through argsBytes/varsBytes/getBytes. Their raw views are read-only; a mutable binding does not grant private fields or mutation APIs.
2. Move Inputs into another owner then use the old binding. Copying requires copy(&inputs); no implicit shared handle exists.
3. Pass constructor source lists to fromParts and use them afterward without copying first. Retained mutable references to any moved input must also reject.
4. Return argsBytes/varsBytes/a raw value derived from a local Inputs, or dispose/move its owner before the view's last use. Duplicating a descriptor preserves its dependencies.
5. With the companion's encoded helper, the conservative key dependency rule deliberately rejects this case. The byte value's backing store is the caller's Inputs, but the rule still retains the key dependency:

   ```rill
   function badLookup(source: &Inputs): Result<Option<&Bytes>, EnvError> {
     let localKey = encoded(&"KEY");
     return getBytes(source, &localKey);
   }
   ```

   Pass a caller-owned key or return owned text instead. No special origin annotation or body-inspection exception is introduced.
6. Treat a strict text query as Option/string without handling its Result, propagate an incompatible error type implicitly, or clone a unique borrowed Err merely to return it.
7. Pass string lists instead of Bytes lists to fromParts, or treat borrowed views as independent Bytes owners. Use explicit existing byte/text conversion/copying.
8. Refer to absent setters/remove-variable APIs, implicit live-process lookup, an executable-path query or a host-inheritance test switch as though this ticket had defined them.
9. Capture a top-level runtime snapshot/configuration inside a named function or expectation. Pass runtime state explicitly; an arrow capture follows ordinary owned/borrowed callable rules. The companion's worker consumes its explicit configuration.
10. Claim a raw borrowed view is noBorrow because Inputs itself is, or erase exclusive/capture evidence to transfer invalid state. Inputs/EnvError's concrete evidence does not erase a contained view's dependency.

## Acquisition, host and cleanup validation

- Native capture occurs at a genuinely safe controlled point, before Rill workers. An already multithreaded embedding supplies owned inputs or coordinates all relevant native access; a Rill-only lock is not proof against foreign mutation. Subsequent operations never consult live native/libc environment state.
- Validate/copy host-reported counts, sizes, terminators, separator bounds and all arithmetic before memory access. A malformed terminated entry is a typed error; an arbitrary invalid pointer, host trap or instantiation failure is not promised recoverable. Explicitly validate the selected provider's sizing/read consistency instead of asserting WASI supplies unspecified guarantees.
- Capture must produce a safe immutable outcome. Failure diagnostics must not retain offending values, and capture must not partially publish Inputs. Repeated acquisition results are independent. Do not clone arbitrary Result/Cause state or run user formatting at bootstrap. Ordinary error origins follow existing construction rules; there is no automatic propagation trace.
- A deliberately empty WASI instance yields empty inputs. WASI sees only supplied instance inputs, not ambient host values. A host accepting only UTF-8 does not establish arbitrary-byte injection; bytes already delivered to the guest remain lossless. No Windows behavior is inferred.
- Changing or poisoning the developer/CI environment must not affect ordinary test inputs. Skip capture entirely in rill test, including when that environment would fail validation. Runner/build flags are not silently passed as program arguments.
- Standard display and retained diagnostic state must omit argument/environment values and arbitrary host diagnostic strings. Test with distinctive secret markers in a NUL value or invalid argument; inspect display/cause output for leaks without freezing human text. Returned category/position stays machine-readable. Explicitly retrieving/printing a value is not automatic redaction, and no secure-zeroization guarantee is made.
- Partial capture, validation, sorting, independent copy and UTF-8 conversion must preserve borrowed inputs until work ends. On error or cancellation, cleanup must dispose every initialized owner exactly once and never publish a partial text list. Allocation failure remains fatal. Standard input-dependent traversal and default cleanup bound their call stacks per #22 without silently truncating data. Allocation-backed worklists are not a new total-memory quota.
- Preserve sources through suspension/budget yields and any actual host/native borrowing. A cancellation request is not confirmed release. #30's native/Wasm stability gates remain; no test here demonstrates those mechanisms.

#28 formalizes the checking and runtime/host rules and converts these cases into executable suites when tooling exists. #24's diagnostic contract governs source diagnostics; #35 owns filesystem paths. If a necessary interface, checking rule or provider mechanism cannot satisfy the confirmed semantics, return that question to the map. Conservative wording and these examples do not establish safety.

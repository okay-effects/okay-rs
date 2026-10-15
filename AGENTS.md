# okay-rs — notes for agents

- Rust 2024 (rustc 1.92), no dependencies; `cargo test` runs the unit, integration and doc tests (the doc tests hold
  the `compile_fail` checks: a row not handled has no `run`; an effect not in the row cannot be performed).
- okay-core's kernel ported (`okay.one` in ~/work/my/okay-core is the specification; the okay-one repository is history): a program is DATA, `Eff<R, A>`, a tree of nodes; its
  effects a row `R` (`Cons<E, R>` … `Nil`, `row![…]`), an effect's place found by the compiler (`Member<E, Ix>`, the
  index `Here`/`There` never written); handlers values of three forms (`Handler::answering`, `stateful`, `deep`), not
  tied to the program; the rest `k` of a deep handler data (`K<T, B>`), resumed by the operation's `Reply`, as many
  times as wanted, each time with its levels as they were when taken — a state's included.
- An operation is an enum variant carrying its `Reply<T>`: `perform(Reader::Ask)`, `perform(|r| State::Put(s, r))`;
  a handler answers only through it (`r.give(v)`, `r.resume(&k, v)`), so the value's type is the operation's.
- The machine (`machine.rs`) is erased: values `Box<dyn Any>`, taken back at the type they were made at — the typed
  API allows no other (Rust has no GADTs: okay-one's typed loop is not portable). An operation goes to the nearest
  level of its effect by `TypeId`; an effect is once in a row (a second would make `Member`'s index ambiguous).
- Frames are `Fn`, not `FnOnce`: a `k` is multi-shot. So `eff! { data; … }` clones the names bound before each rest — a name
  bound is `Clone`.
- Tests are okay-core's kernel's, named after them (`tests/std.rs` = TestStd, `tests/acceptance.rs` = TestAcceptance); port the
  next ones the same way.
- INTERCEPTORS, the point of it all — control and audit of effects (`tests/intercept.rs` is the specification, the same programs as okay-core's TestIntercept):
  - an effect is `Plain` or `Sensitive` (`Effect::Kind`). A sensitive one is handled only by `handle_audited`, which
    needs `Audited<S>` in the row; only `audit` (an `Interceptor` of `S`) or `observe` puts it there, in the row
    outside them — where no program can perform `S`. A plain `handle`/`intercept` of it does not compile.
  - `intercept`: a plain effect handled again for a part of a program, its clauses outside it (their own operations
    reach the handler further out). `audit`: the same for a sensitive one, its clauses given `Pass<S>`, the only way
    to perform `S` past the audit — each operation told, passed on changed, or refused.
  - `observe`: an `Observer` sees every operation of every effect on its way out (`AnyOp`: its effect, `Debug`
    description), passes it on (`proceed`, `proceed_then` telling its answer each time the rest gets it) or refuses
    it. So an operation's value is `Clone + Debug`, and an effect's enum too.
  - the trail is an effect, `audit::Audit` (`record`, handled by `trail()`), outside the interceptors.
- Recursion: `call!(f(args))` (a tail call, `defer` underneath) and `tail_rec(s0, step)` (`Loop::Next`/`Done`) run
  any depth on the machine's heap stack (`tests/recursion.rs`, a million steps). A recursive call before its
  `flat_map` is Rust's own recursion: keep it inside a closure.
- `okay::typed` — ONE API compiled by the handlers' types: `eff! { … }` is an `async` block over a context `cx`
  (`Cx(h, rest)`, `Has<E, Ix>`), a program as data the machine's `eff! { data; … }`. An operation goes through each
  layer of the context (`Has::perform`/`resume`, typed by its value): `Paths` (`all`/`first`) logs the answers from
  further out and replays them (many-shot `k` by replay — the body an `AsyncFn` deciding by its effects only),
  `Observed` (`observe`) shows each to a `Watch`. Interceptors: `Audited::new(handler, interceptor)`, an
  `Interceptor<E, R, Ix>` passing each operation on (`Pass`), changing or refusing it, its own operations to the
  context after the handler (`Has<E, Through<Ix>>`); a sensitive effect has no other way into a context (`Allowed`,
  sealed). `tests/typed_intercept.rs` = `tests/intercept.rs`'s programs and trails: keep the three in step. `tail_rec` for recursion in constant stack. `effect!` declares an
  effect and its operations for both forms; `needs!` an async function over a context, its effects listed (`cx:
  [Reader<i64>, State<i64>]`): the context's type and an index per effect made (a pool of 16 names, `I0`…`I15`). `tests/replay.rs` checks the replay against the machine's values.
- Benchmarks: `benches/kernel.rs`, okay-core's KernelBenchmark lanes, the same programs and n; run it under JMH's
  lock (`/tmp/okz/tools/withlock.py`) — other sessions measure on this machine — and compile under it too.
- Stage 2 (not yet): `Cont<R, I, O, A>` for answer-type modification — `$`/`reset`, `shift0`, `PState` — with
  `Bind` impls for the four pairs of `Eff`/`Cont` (no overlap, so no specialization); `on_unimplemented` messages.
- No casts outside `take`/`take_op`/`state` (the erased machine's three); no panics but theirs and the machine's
  "a row not checked", which a typed program never reaches.
- Commits: no Co-Authored-By lines.

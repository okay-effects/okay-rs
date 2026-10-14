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
- Frames are `Fn`, not `FnOnce`: a `k` is multi-shot. So `eff!` clones the names bound before each rest — a name
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
- Stage 2 (not yet): `Cont<R, I, O, A>` for answer-type modification — `$`/`reset`, `shift0`, `PState` — with
  `Bind` impls for the four pairs of `Eff`/`Cont` (no overlap, so no specialization); `on_unimplemented` messages.
- No casts outside `take`/`take_op`/`state` (the erased machine's three); no panics but theirs and the machine's
  "a row not checked", which a typed program never reaches.
- Commits: no Co-Authored-By lines.

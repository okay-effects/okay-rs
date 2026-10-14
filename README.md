# okay-rs

okay-one in Rust: a program of effects is data (`Eff<R, A>`), its effects a row of types, its handlers values not
tied to it, and the rest `k` data too: resumed any number of times, each time as it was taken.

```rust
use okay::*;
use okay::effects::*;

type R = row![Reader<i64>, State<i64>];
let p: Eff<R, i64> = eff! {
    n <- ask::<i64, _, _>();
    _ <- modify::<i64, _, _>(move |s| s + n);
    s <- get::<i64, _, _>();
    yield s * 2
};
assert_eq!(p.handle(reader(5)).handle(state(1)).run(), (6, 12));
```

Stage 1: rows, `Eff`, the three handler forms, reader/state/writer/choose/throws, the machine, `eff!`; okay-one's
TestStd and TestAcceptance pass. Stage 2: answer-type modification (`Cont`, `$`, `shift0`, `PState`).

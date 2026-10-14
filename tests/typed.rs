//! ONE API COMPILED BY THE HANDLER'S TYPE (okay::typed): programs written once with `eff! { async; … }`, run under
//! handlers answering in place, dropping the rest, taking it once; a sensitive effect only under its audit

use std::cell::RefCell;

use okay::eff;
use okay::effects::{Reader, State, Throws, Writer};
use okay::typed::*;
use okay::{Effect, Reply, Sensitive};

async fn counted<C, I1, I2, I3>(cx: &C, n: i64) -> i64
where
    C: Has<Reader<i64>, I1> + Has<State<i64>, I2> + Has<Writer<String>, I3>,
{
    eff! { async;
        d <- ask::<i64, _, _>(cx);
        _ <- modify::<i64, _, _>(cx, |s| s + d * n);
        s <- get::<i64, _, _>(cx);
        _ <- tell::<String, _, _>(cx, format!("s {s}"));
        yield s
    }
    .await
}

/// a writer of another type: nothing kept — the same program compiled for it
struct Silent;
impl Handles<Writer<String>> for Silent {
    fn handle(&self, op: Writer<String>) -> Step {
        match op {
            Writer::Tell(_, r) => Step::Now(r.give(())),
        }
    }
}

/// "one program, two contexts: compiled for each"
#[test]
fn one_program_two_contexts_compiled_for_each() {
    let kept = Cx(AskWith(2i64), Cx(StateCell::new(1i64), Cx(Collect::<String>::new(), End)));
    assert_eq!(run(counted(&kept, 10)), Ok(21));
    assert_eq!(kept.1.1.0.take(), vec!["s 21".to_string()]);
    let silent = Cx(AskWith(5i64), Cx(StateCell::new(0i64), Cx(Silent, End)));
    assert_eq!(run(counted(&silent, 2)), Ok(10));
}

/// "either: the error raised, the rest of the body dropped"
#[test]
fn either_the_error_raised_the_rest_of_the_body_dropped() {
    let cx = Cx(StateCell::new(0i64), End);
    let after = RefCell::new(false);
    let r: Result<Result<i64, String>, Stuck> = run(either(&cx, async |cx| {
        put::<i64, _, _>(cx, 1).await;
        let _: () = raise::<String, (), _, _>(cx, "no".to_string()).await;
        *after.borrow_mut() = true;
        7
    }));
    assert_eq!(r, Ok(Err("no".to_string())));
    assert!(!*after.borrow(), "the rest after the raise ran");
    assert_eq!(cx.0.take(), 1, "the state before the raise kept");
}

/// "a raise in a loop, each under its own either: the loop goes on"
#[test]
fn a_raise_in_a_loop_each_under_its_own_either() {
    let cx = End;
    let total = run(async {
        let mut acc = 0;
        for i in 0..10 {
            let r: Result<i64, String> = either(&cx, async |cx| {
                if i % 2 == 0 { raise::<String, i64, _, _>(cx, "even".to_string()).await } else { i }
            })
            .await;
            acc += r.unwrap_or(100);
        }
        acc
    });
    assert_eq!(total, Ok(5 * 100 + (1 + 3 + 5 + 7 + 9)));
}

/// "a generator: each value given as it comes, the body suspended at each emit and resumed once — a state in it"
#[test]
fn a_generator_each_value_as_it_comes_the_body_resumed_once() {
    let cx = Cx(StateCell::new(0i64), End);
    let seen = RefCell::new(Vec::new());
    let done = run(generate(&cx, |v: i64| seen.borrow_mut().push(v), async |cx| {
        for i in 1..=5 {
            modify::<i64, _, _>(cx, |s| s + i).await;
            let s = get::<i64, _, _>(cx).await;
            emit::<i64, _, _>(cx, s).await;
        }
        "done"
    }));
    assert_eq!(done, Ok("done"));
    assert_eq!(*seen.borrow(), vec![1, 3, 6, 10, 15]);
}

// A SENSITIVE EFFECT: payments, under their audit only

#[derive(Clone, Debug)]
enum Pay {
    Transfer(String, i64, Reply<Result<String, String>>),
}
impl Effect for Pay {
    type Kind = Sensitive;
}

fn transfer<'c, C: Has<Pay, I>, I>(cx: &'c C, to: &str, amount: i64) -> Perform<'c, C, Pay, I, Result<String, String>> {
    let to = to.to_string();
    perform(cx, |r| Pay::Transfer(to, amount, r))
}

struct Bank(RefCell<i64>);

/// the audit's policy: a limit, the trail
struct Guard {
    limit: i64,
    trail: RefCell<Vec<String>>,
}

impl Handles<Pay> for Audited<Bank, Guard> {
    fn handle(&self, op: Pay) -> Step {
        match op {
            Pay::Transfer(to, amount, r) => {
                let to = to.trim().to_uppercase();
                if amount > self.interceptor.limit {
                    let why = format!("refused: {amount} over the limit {}", self.interceptor.limit);
                    self.interceptor.trail.borrow_mut().push(why.clone());
                    return Step::Now(r.give(Err(why)));
                }
                *self.handler.0.borrow_mut() -= amount;
                let receipt = format!("ok:{to}:{amount}");
                self.interceptor.trail.borrow_mut().push(format!("transfer {amount} → {receipt}"));
                Step::Now(r.give(Ok(receipt)))
            }
        }
    }
}

/// "a sensitive effect under its audit: passed on changed, told, refused over the limit"
#[test]
fn a_sensitive_effect_under_its_audit() {
    let cx = Cx(
        Audited { handler: Bank(RefCell::new(1000)), interceptor: Guard { limit: 1000, trail: RefCell::new(vec![]) } },
        End,
    );
    let c = &cx; // `eff! { async; … }` moves what it uses: a reference to the context
    let r: Result<Result<String, String>, Stuck> = run(eff! { async;
        a <- transfer(c, " acct-1 ", 100);
        b <- transfer(c, "acct-2", 5000);
        yield a.and(b)
    });
    assert_eq!(r, Ok(Err("refused: 5000 over the limit 1000".to_string())));
    assert_eq!(
        cx.0.interceptor.trail.borrow().clone(),
        vec!["transfer 100 → ok:ACCT-1:100".to_string(), "refused: 5000 over the limit 1000".to_string()]
    );
    assert_eq!(*cx.0.handler.0.borrow(), 900);
}

/// "a raise reaches the nearest either: the types find it"
#[test]
fn throws_found_by_type() {
    fn _typed<C: Has<Throws<String>, I>, I>(_: &C) {}
    let cx = End;
    let _: Result<Result<i32, String>, Stuck> = run(either(&cx, async |cx| {
        _typed(cx);
        0
    }));
}

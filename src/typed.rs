//! ONE API, COMPILED BY THE HANDLER'S TYPE: a program written with `eff! { … }` is an `async` block — a state
//! machine the compiler builds, generic over its context `cx`, the handlers in force. Monomorphized for the handlers it
//! runs under: an operation of a handler answering in place is `Ready` at once — inlined, a call of the handler; one
//! dropping the rest ends the block (`either`); one taking the rest once suspends it — the state machine itself the
//! `k`, no node on the heap (`generate`); one taking it many times runs it again for each path, its past told it
//! again (`all`, `first`: replay). A sensitive effect is in a context only under its audit; every operation of every
//! effect seen by a `Watch` under `observe`.
//!
//! ```
//! use okay::eff;
//! use okay::effects::{Reader, State, Writer};
//! use okay::typed::*;
//!
//! async fn counted<C, I1, I2, I3>(cx: &C, n: i64) -> i64
//! where C: Has<Reader<i64>, I1> + Has<State<i64>, I2> + Has<Writer<String>, I3> {
//!     eff! {
//!         d <- ask(cx);
//!         _ <- modify(cx, |s| s + d * n);
//!         s <- get(cx);
//!         _ <- tell(cx, format!("s {s}"));
//!         yield s
//!     }.await
//! }
//!
//! let cx = Cx(AskWith(2i64), Cx(StateCell::new(1i64), Cx(Collect::<String>::new(), End)));
//! assert_eq!(run(counted(&cx, 10)), Ok(21));
//! assert_eq!(cx.1.1.0.take(), vec!["s 21".to_string()]);
//! ```
//!
//! A sensitive effect's bare handler is not a context's:
//!
//! ```compile_fail
//! use okay::typed::*;
//! use okay::{Effect, Reply, Sensitive};
//! #[derive(Clone, Debug)] enum Pay { Transfer(i64, Reply<String>) }
//! impl Effect for Pay { type Kind = Sensitive; }
//! struct Bank;
//! impl Handles<Pay> for Bank {
//!     fn handle(&self, op: Pay) -> Step { match op { Pay::Transfer(n, r) => Step::Now(r.give(format!("ok {n}"))) } }
//! }
//! let cx = Cx(Bank, End);
//! let _ = run(perform::<_, Pay, _, String>(&cx, |r| Pay::Transfer(5, r)));
//! ```

use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::{pin, Pin};
use std::task::{Context, Poll, Waker};

use crate::eff::{short, Answer, Loop, Never, Reply};
use crate::effects::{Choose, Reader, State, Throws, Writer};
use crate::val::Val;
use crate::row::{Effect, Here, Plain, Sensitive, There};

// HANDLERS

/// what a handler does with an operation: answers it now, or takes the rest (the program suspended)
pub enum Step {
    Now(Answer),
    Later,
}

/// A HANDLER OF `E` in a context: `handle` each operation; an operation it took (`Later`) answered by `resume` when it
/// gives the rest back — once
pub trait Handles<E: Effect> {
    fn handle(&self, op: E) -> Step;
    fn resume(&self) -> Option<Answer> {
        None
    }
}

mod sealed {
    /// a type no one outside okay names: what `Allowed`'s method asks
    pub struct Token;
}

/// A HANDLER ALLOWED FOR AN EFFECT OF THE KIND `K`: any handler for a plain effect; for a sensitive one okay's audit
/// alone. Sealed: its method asks a type no one outside okay can name
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not under an audit: a sensitive effect is handled only through one",
    label = "not an audit",
    note = "put `Audited {{ handler, interceptor }}` in the context, not the bare handler"
)]
pub trait Allowed<K> {
    #[doc(hidden)]
    fn allowed(&self, _: sealed::Token);
}

impl<H> Allowed<Plain> for H {
    fn allowed(&self, _: sealed::Token) {}
}

/// A HANDLER UNDER ITS AUDIT: the effect's author implements `Handles<E>` for it, calling the interceptor around the
/// handler — the only way a sensitive effect is handled
pub struct Audited<H, I> {
    pub handler: H,
    pub interceptor: I,
}

impl<H, I> Allowed<Sensitive> for Audited<H, I> {
    fn allowed(&self, _: sealed::Token) {}
}

// THE CONTEXT: the handlers in force, a list — `Cx(h, rest)`, `End`

pub struct Cx<H, R>(pub H, pub R);
pub struct End;

/// `E` HANDLED IN THE CONTEXT `Self`, at `Ix` — found by the compiler, as `Member`: an operation given to its handler
/// — on the way through each layer between (a replay, an observer)
#[diagnostic::on_unimplemented(
    message = "no handler of `{E}` in the context `{Self}`",
    note = "put a handler of `{E}` in the context; a sensitive effect's only under its audit: `Audited {{ handler, interceptor }}`"
)]
pub trait Has<E: Effect, Ix> {
    /// the operation's value now, or none: taken, the program suspended
    fn perform<T: Clone + Debug + 'static>(&self, op: E) -> Option<T>;
    /// the operation taken given back: its value, or none — taken still
    fn resume<T: Clone + Debug + 'static>(&self) -> Option<T>;
}

impl<E: Effect, H: Handles<E> + Allowed<E::Kind>, R> Has<E, Here> for Cx<H, R> {
    #[inline(always)]
    fn perform<T: Clone + Debug + 'static>(&self, op: E) -> Option<T> {
        match self.0.handle(op) {
            Step::Now(a) => Some(a.0.take::<T>()),
            Step::Later => None,
        }
    }
    #[inline(always)]
    fn resume<T: Clone + Debug + 'static>(&self) -> Option<T> {
        self.0.resume().map(|a| a.0.take::<T>())
    }
}

impl<E: Effect, H, R: Has<E, Ix>, Ix> Has<E, There<Ix>> for Cx<H, R> {
    #[inline(always)]
    fn perform<T: Clone + Debug + 'static>(&self, op: E) -> Option<T> {
        self.1.perform(op)
    }
    #[inline(always)]
    fn resume<T: Clone + Debug + 'static>(&self) -> Option<T> {
        self.1.resume()
    }
}

impl<E: Effect, Ix, C: Has<E, Ix>> Has<E, Ix> for &C {
    #[inline(always)]
    fn perform<T: Clone + Debug + 'static>(&self, op: E) -> Option<T> {
        (**self).perform(op)
    }
    #[inline(always)]
    fn resume<T: Clone + Debug + 'static>(&self) -> Option<T> {
        (**self).resume()
    }
}

// AN OPERATION: a future — answered at once by a handler in place, else pending until its handler resumes it

pub struct Perform<'c, C, E, Ix, T> {
    cx: &'c C,
    op: Option<E>,
    _t: PhantomData<fn() -> (Ix, T)>,
}

impl<'c, C, E, Ix, T> Perform<'c, C, E, Ix, T> {
    #[inline(always)]
    pub fn new(cx: &'c C, op: E) -> Self {
        Perform { cx, op: Some(op), _t: PhantomData }
    }
}

// not pinned by anything in it: no field refers to another
impl<C, E, Ix, T> Unpin for Perform<'_, C, E, Ix, T> {}

impl<C: Has<E, Ix>, E: Effect, Ix, T: Clone + Debug + 'static> Future for Perform<'_, C, E, Ix, T> {
    type Output = T;
    #[inline(always)]
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        let me = self.get_mut();
        let t = match me.op.take() {
            Some(op) => me.cx.perform::<T>(op),
            None => me.cx.resume::<T>(),
        };
        match t {
            Some(t) => Poll::Ready(t),
            None => Poll::Pending,
        }
    }
}

/// an operation of `E` in the context: `make` its variant given its reply
#[inline(always)]
pub fn perform<'c, C: Has<E, Ix>, E: Effect, Ix, T: Clone + Debug + 'static>(
    cx: &'c C,
    make: impl FnOnce(Reply<T>) -> E,
) -> Perform<'c, C, E, Ix, T> {
    Perform::new(cx, make(Reply::new()))
}

// THE STANDARD EFFECTS' OPERATIONS — the machine's effects, their enums the same

#[inline(always)]
pub fn ask<V: Clone + std::fmt::Debug + 'static, C: Has<Reader<V>, I>, I>(cx: &C) -> Perform<'_, C, Reader<V>, I, V> {
    perform(cx, Reader::Ask)
}
#[inline(always)]
pub fn get<S: Clone + std::fmt::Debug + 'static, C: Has<State<S>, I>, I>(cx: &C) -> Perform<'_, C, State<S>, I, S> {
    perform(cx, State::Get)
}
#[inline(always)]
pub fn put<S: Clone + std::fmt::Debug + 'static, C: Has<State<S>, I>, I>(cx: &C, s: S) -> Perform<'_, C, State<S>, I, ()> {
    perform(cx, |r| State::Put(s, r))
}
/// the state moved by `f`
pub async fn modify<S: Clone + std::fmt::Debug + 'static, C: Has<State<S>, I>, I>(cx: &C, f: impl FnOnce(S) -> S) {
    let s = get::<S, C, I>(cx).await;
    put::<S, C, I>(cx, f(s)).await
}
#[inline(always)]
pub fn tell<W: Clone + std::fmt::Debug + 'static, C: Has<Writer<W>, I>, I>(cx: &C, w: W) -> Perform<'_, C, Writer<W>, I, ()> {
    perform(cx, |r| Writer::Tell(w, r))
}
/// an error raised: the rest dropped by the handler of `Throws<E>`
pub async fn raise<E: Clone + std::fmt::Debug + 'static, A, C: Has<Throws<E>, I>, I>(cx: &C, e: E) -> A {
    let never: crate::Never = perform(cx, |r| Throws::Raise(e, r)).await;
    match never {}
}

// THEIR HANDLERS

/// each ask answered the same value
pub struct AskWith<V>(pub V);
impl<V: Clone + std::fmt::Debug + 'static> Handles<Reader<V>> for AskWith<V> {
    #[inline(always)]
    fn handle(&self, op: Reader<V>) -> Step {
        match op {
            Reader::Ask(r) => Step::Now(r.give(self.0.clone())),
        }
    }
}

/// a state in place
pub struct StateCell<S>(pub RefCell<S>);
impl<S> StateCell<S> {
    pub fn new(s: S) -> Self {
        StateCell(RefCell::new(s))
    }
    pub fn take(self) -> S {
        self.0.into_inner()
    }
}
impl<S: Clone + std::fmt::Debug + 'static> Handles<State<S>> for StateCell<S> {
    #[inline(always)]
    fn handle(&self, op: State<S>) -> Step {
        match op {
            State::Get(r) => Step::Now(r.give(self.0.borrow().clone())),
            State::Put(s, r) => {
                *self.0.borrow_mut() = s;
                Step::Now(r.give(()))
            }
        }
    }
}

/// the told, in order
pub struct Collect<W>(pub RefCell<Vec<W>>);
impl<W> Collect<W> {
    pub fn new() -> Self {
        Collect(RefCell::new(Vec::new()))
    }
    pub fn take(&self) -> Vec<W> {
        std::mem::take(&mut self.0.borrow_mut())
    }
}
impl<W> Default for Collect<W> {
    fn default() -> Self {
        Self::new()
    }
}
impl<W: Clone + std::fmt::Debug + 'static> Handles<Writer<W>> for Collect<W> {
    #[inline(always)]
    fn handle(&self, op: Writer<W>) -> Step {
        match op {
            Writer::Tell(w, r) => {
                self.0.borrow_mut().push(w);
                Step::Now(r.give(()))
            }
        }
    }
}

// DROPPING THE REST: either — the body's value, or the error raised in it, the body dropped there

/// the handler of `Throws<E>` that `either` puts in the context: the error kept, the rest taken (never given back)
pub struct Raised<E>(RefCell<Option<E>>);
impl<E: Clone + std::fmt::Debug + 'static> Handles<Throws<E>> for Raised<E> {
    #[inline(always)]
    fn handle(&self, op: Throws<E>) -> Step {
        match op {
            Throws::Raise(e, _) => {
                *self.0.borrow_mut() = Some(e);
                Step::Later
            }
        }
    }
}

/// `body` under a handler of `Throws<E>`: its value, or the error raised — the body dropped where it raised
pub async fn either<'c, E, A, C>(cx: &'c C, body: impl AsyncFnOnce(&Cx<Raised<E>, &'c C>) -> A) -> Result<A, E>
where
    E: Clone + std::fmt::Debug + 'static,
{
    let inner = Cx(Raised(RefCell::new(None)), cx);
    let mut fut = pin!(body(&inner));
    std::future::poll_fn(|c| match fut.as_mut().poll(c) {
        Poll::Ready(a) => Poll::Ready(Ok(a)),
        Poll::Pending => match inner.0.0.borrow_mut().take() {
            Some(e) => Poll::Ready(Err(e)),
            None => Poll::Pending,
        },
    })
    .await
}

// TAKING THE REST ONCE: a generator — each value emitted to the consumer, the body resumed after it

/// a value emitted
#[derive(Clone, Debug)]
pub enum Emit<V> {
    Value(V, Reply<()>),
}
impl<V: Clone + std::fmt::Debug + 'static> Effect for Emit<V> {
    type Kind = Plain;
}

#[inline(always)]
pub fn emit<V: Clone + std::fmt::Debug + 'static, C: Has<Emit<V>, I>, I>(cx: &C, v: V) -> Perform<'_, C, Emit<V>, I, ()> {
    perform(cx, |r| Emit::Value(v, r))
}

/// the handler `generate` puts in the context: the value emitted kept, the body suspended until it is taken
pub struct Emitted<V> {
    slot: RefCell<Option<V>>,
    taken: Cell<bool>,
}
impl<V: Clone + std::fmt::Debug + 'static> Handles<Emit<V>> for Emitted<V> {
    #[inline(always)]
    fn handle(&self, op: Emit<V>) -> Step {
        match op {
            Emit::Value(v, _) => {
                *self.slot.borrow_mut() = Some(v);
                self.taken.set(false);
                Step::Later
            }
        }
    }
    #[inline(always)]
    fn resume(&self) -> Option<Answer> {
        if self.taken.get() { Some(Reply::<()>::new().give(())) } else { None }
    }
}

/// every value the body emits, given to `each` as it comes — the body suspended at each emit, resumed after it: its
/// rest taken once, the state machine itself
pub async fn generate<'c, V, A, C>(
    cx: &'c C,
    mut each: impl FnMut(V),
    body: impl AsyncFnOnce(&Cx<Emitted<V>, &'c C>) -> A,
) -> A
where
    V: Clone + std::fmt::Debug + 'static,
{
    let inner = Cx(Emitted { slot: RefCell::new(None), taken: Cell::new(false) }, cx);
    let mut fut = pin!(body(&inner));
    std::future::poll_fn(|c| loop {
        match fut.as_mut().poll(c) {
            Poll::Ready(a) => return Poll::Ready(a),
            Poll::Pending => match inner.0.slot.borrow_mut().take() {
                Some(v) => {
                    each(v);
                    inner.0.taken.set(true);
                }
                None => return Poll::Pending,
            },
        }
    })
    .await
}

// TAKING THE REST MANY TIMES: by replay — the body run again for each path, its past told it again

/// what a path was told, in order: a choice (its index, `among` alternatives), or an answer from further out (`among`
/// 0)
struct Told {
    val: Val,
    among: usize,
}

/// the layer `all` and `first` put in the context: the choices of the path now, and the answers from further out told
/// before the last of them — told again, not asked again, when the body runs again (Koppel et al., "Capturing the
/// future by replaying the past", ICFP 2018)
pub struct Paths<'c, C> {
    told: RefCell<Vec<Told>>,
    at: Cell<usize>,
    failed: Cell<bool>,
    outer: &'c C,
}

impl<'c, C> Paths<'c, C> {
    fn new(outer: &'c C) -> Self {
        Paths { told: RefCell::new(Vec::new()), at: Cell::new(0), failed: Cell::new(false), outer }
    }

    /// the value told here before, if the body is replaying its past
    #[inline]
    fn replayed<T: Clone + 'static>(&self) -> Option<T> {
        let at = self.at.get();
        let told = self.told.borrow();
        let t = told.get(at)?;
        self.at.set(at + 1);
        Some(t.val.peek::<T>().cloned().unwrap_or_else(|| not_replayed()))
    }

    #[inline]
    fn tell<T: Clone + 'static>(&self, t: &T, among: usize) {
        self.told.borrow_mut().push(Told { val: Val::new(t.clone()), among });
        self.at.set(self.at.get() + 1);
    }

    /// the next path: the last choice with an alternative left moved to it, all told after it forgotten; none left —
    /// false
    fn next(&self) -> bool {
        let mut told = self.told.borrow_mut();
        while let Some(t) = told.pop() {
            if t.among > 0 {
                let i = t.val.take::<usize>() + 1;
                if i < t.among {
                    told.push(Told { val: Val::new(i), among: t.among });
                    return true;
                }
            }
        }
        false
    }

    /// the body run along the path told: its value, or none — the path failed
    async fn path<A>(&self, body: &impl AsyncFn(&Self) -> A) -> Option<A> {
        self.at.set(0);
        self.failed.set(false);
        let mut fut = pin!(body(self));
        std::future::poll_fn(|c| match fut.as_mut().poll(c) {
            Poll::Ready(a) => Poll::Ready(Some(a)),
            Poll::Pending if self.failed.get() => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        })
        .await
    }
}

#[cold]
fn not_replayed() -> ! {
    panic!("okay: the body did not replay its past — it decides by something not an effect (a cell, the time)")
}

impl<C> Has<Choose, Here> for Paths<'_, C> {
    #[inline]
    fn perform<T: Clone + Debug + 'static>(&self, op: Choose) -> Option<T> {
        match op {
            Choose::Among(n, r) => {
                let i = match self.replayed::<usize>() {
                    Some(i) => i,
                    None if n == 0 => {
                        self.failed.set(true);
                        return None;
                    }
                    None => {
                        self.tell(&0usize, n);
                        0
                    }
                };
                Some(r.give(i).0.take::<T>())
            }
            Choose::Fail(_) => {
                self.failed.set(true);
                None
            }
        }
    }
    fn resume<T: Clone + Debug + 'static>(&self) -> Option<T> {
        None
    }
}

impl<E: Effect, C: Has<E, Ix>, Ix> Has<E, There<Ix>> for Paths<'_, C> {
    #[inline]
    fn perform<T: Clone + Debug + 'static>(&self, op: E) -> Option<T> {
        if let Some(t) = self.replayed::<T>() {
            return Some(t);
        }
        let t = self.outer.perform::<T>(op)?;
        self.tell(&t, 0);
        Some(t)
    }
    #[inline]
    fn resume<T: Clone + Debug + 'static>(&self) -> Option<T> {
        let t = self.outer.resume::<T>()?;
        self.tell(&t, 0);
        Some(t)
    }
}

/// one of the values, each in turn
pub async fn among<A: Clone, C: Has<Choose, I>, I>(cx: &C, xs: &[A]) -> A {
    let i: usize = perform(cx, |r| Choose::Among(xs.len(), r)).await;
    xs[i].clone()
}

/// true, then false
pub async fn flip<C: Has<Choose, I>, I>(cx: &C) -> bool {
    let i: usize = perform(cx, |r| Choose::Among(2, r)).await;
    i == 0
}

/// no value: this path dropped
pub async fn fail<A, C: Has<Choose, I>, I>(cx: &C) -> A {
    let never: Never = perform(cx, Choose::Fail).await;
    match never {}
}

/// every value of the body, one per path, in order — the body run again for each, its past told it again: the
/// choices made, the answers from further out (each asked once: a state further out threaded through the paths in
/// order, as the machine's `all` under it). So the body decides by its effects only, and is an `AsyncFn`
pub async fn all<'c, A, C>(cx: &'c C, body: impl AsyncFn(&Paths<'c, C>) -> A) -> Vec<A> {
    let paths = Paths::new(cx);
    let mut out = Vec::new();
    loop {
        if let Some(a) = paths.path(&body).await {
            out.push(a);
        }
        if !paths.next() {
            return out;
        }
    }
}

/// the value of the first path not failing, the rest not run
pub async fn first<'c, A, C>(cx: &'c C, body: impl AsyncFn(&Paths<'c, C>) -> A) -> Option<A> {
    let paths = Paths::new(cx);
    loop {
        if let Some(a) = paths.path(&body).await {
            return Some(a);
        }
        if !paths.next() {
            return None;
        }
    }
}

// RECURSION

/// A LOOP IN CONSTANT STACK: `step` from a state to the next — `Loop::Next(s)` on, `Loop::Done(a)` its value; a
/// recursion, a mutual one (its calls the states: `Call::Even(n)`, `Call::Odd(n)`), at any depth. An async recursion
/// is Rust's own: each call a future inside its caller's (boxed), polled through all of them — deep, it overflows
pub async fn tail_rec<S, A>(s0: S, step: impl AsyncFn(S) -> Loop<S, A>) -> A {
    let mut s = s0;
    loop {
        match step(s).await {
            Loop::Next(next) => s = next,
            Loop::Done(a) => return a,
        }
    }
}

// EVERY OPERATION OF EVERY EFFECT: a watch between the body and its handlers

/// sees each operation on its way out — its effect (`Reader<i64>`), the operation as written (`Put(6, _)`) — lets it
/// through or refuses it (the body dropped there), then sees its answer
pub trait Watch {
    fn before(&self, effect: &str, op: &dyn Debug) -> Result<(), String> {
        let _ = (effect, op);
        Ok(())
    }
    fn after(&self, effect: &str, op: &dyn Debug, answer: &dyn Debug) {
        let _ = (effect, op, answer);
    }
}

impl<W: Watch> Watch for &W {
    fn before(&self, effect: &str, op: &dyn Debug) -> Result<(), String> {
        (**self).before(effect, op)
    }
    fn after(&self, effect: &str, op: &dyn Debug, answer: &dyn Debug) {
        (**self).after(effect, op, answer)
    }
}

/// the layer `observe` puts in the context: no handler of its own, each effect at the index it has further out
pub struct Observed<'c, W, C> {
    watch: W,
    refused: RefCell<Option<String>>,
    waiting: RefCell<Option<(String, Box<dyn Debug>)>>,
    outer: &'c C,
}

impl<E: Effect, Ix, W: Watch, C: Has<E, Ix>> Has<E, Ix> for Observed<'_, W, C> {
    fn perform<T: Clone + Debug + 'static>(&self, op: E) -> Option<T> {
        let effect = short(std::any::type_name::<E>());
        if let Err(why) = self.watch.before(&effect, &op) {
            *self.refused.borrow_mut() = Some(why);
            return None;
        }
        let seen = op.clone();
        match self.outer.perform::<T>(op) {
            Some(t) => {
                self.watch.after(&effect, &seen, &t);
                Some(t)
            }
            None => {
                *self.waiting.borrow_mut() = Some((effect, Box::new(seen)));
                None
            }
        }
    }
    fn resume<T: Clone + Debug + 'static>(&self) -> Option<T> {
        let t = self.outer.resume::<T>()?;
        if let Some((effect, seen)) = self.waiting.borrow_mut().take() {
            self.watch.after(&effect, &*seen, &t);
        }
        Some(t)
    }
}

/// `body` with every operation of every effect seen by `watch` on its way out: its value, or why the watch refused
/// an operation — the body dropped there
pub async fn observe<'c, W: Watch, A, C>(
    cx: &'c C,
    watch: W,
    body: impl AsyncFnOnce(&Observed<'c, W, C>) -> A,
) -> Result<A, String> {
    let inner = Observed { watch, refused: RefCell::new(None), waiting: RefCell::new(None), outer: cx };
    let mut fut = pin!(body(&inner));
    std::future::poll_fn(|c| match fut.as_mut().poll(c) {
        Poll::Ready(a) => Poll::Ready(Ok(a)),
        Poll::Pending => match inner.refused.borrow_mut().take() {
            Some(why) => Poll::Ready(Err(why)),
            None => Poll::Pending,
        },
    })
    .await
}

// RUNNING

/// why a run did not end: an operation taken by a handler that never gives it back, with none outside to drop it
#[derive(Debug, PartialEq, Eq)]
pub struct Stuck;

/// a program run to its value — every handler of its context in force
pub fn run<F: Future>(fut: F) -> Result<F::Output, Stuck> {
    let mut fut = pin!(fut);
    let mut c = Context::from_waker(Waker::noop());
    match fut.as_mut().poll(&mut c) {
        Poll::Ready(a) => Ok(a),
        Poll::Pending => Err(Stuck),
    }
}
